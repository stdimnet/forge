//! Multi-peer simulation: a host with a `LocalBackend` on a temporary folder and simulated
//! guests over the in-memory network. Each participant's editor is a `yrs` replica standing
//! in for the UI's Yjs document.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_backend::{Backend, LocalBackend};
use forge_collab::excludes::Excludes;
use forge_collab::guest::{GuestEvent, GuestSession};
use forge_collab::host::{DevAutoAccept, HostConfig, HostEvent, HostSession, DEV_PREVIEW_NOTICE};
use forge_collab::link::SessionLink;
use forge_collab::permissions::Permissions;
use forge_collab::presence::{Caret, ParticipantId, Presence, Viewport};
use forge_collab::protocol::{text_hash, DocId, SaveState};
use forge_collab::reconnect::ReconnectPolicy;
use forge_collab::transport::memory::MemoryNetwork;
use forge_core::FsChanges;
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{
    Doc, GetString, OffsetKind, Options, ReadTxn, StateVector, Text, TextRef, Transact, Update,
};

/// A participant's editor: one replica per open document, and what arrived for it.
#[derive(Default)]
struct Editor {
    docs: HashMap<DocId, (Doc, TextRef)>,
    early: HashMap<DocId, Vec<Vec<u8>>>,
    saved: HashMap<DocId, SaveState>,
    renamed: Vec<(DocId, String)>,
    closed: Vec<(DocId, String)>,
    denied: Vec<String>,
    notices: Vec<String>,
    ended: Option<String>,
    /// Where the others are, as this participant was told.
    presence: HashMap<ParticipantId, Presence>,
    follow_host: bool,
    /// How often the connection was restored.
    resumed: usize,
}

type Shared = Arc<Mutex<Editor>>;

impl Editor {
    fn receive(&mut self, doc: DocId, update: &[u8]) {
        match self.docs.get(&doc) {
            Some((d, _)) => d
                .transact_mut()
                .apply_update(Update::decode_v1(update).unwrap())
                .unwrap(),
            None => self.early.entry(doc).or_default().push(update.to_vec()),
        }
    }

    fn open(&mut self, doc: DocId, state: &[u8]) {
        let d = Doc::with_options(Options {
            offset_kind: OffsetKind::Utf16,
            ..Options::default()
        });
        let t = d.get_or_insert_text("content");
        d.transact_mut()
            .apply_update(Update::decode_v1(state).unwrap())
            .unwrap();
        for u in self.early.remove(&doc).unwrap_or_default() {
            d.transact_mut()
                .apply_update(Update::decode_v1(&u).unwrap())
                .unwrap();
        }
        self.docs.insert(doc, (d, t));
    }

    fn text(&self, doc: DocId) -> String {
        let (d, t) = &self.docs[&doc];
        t.get_string(&d.transact())
    }

    /// Type at a position (clamped); returns the update to send.
    fn edit(&self, doc: DocId, at: u32, delete: u32, insert: &str) -> Vec<u8> {
        let (d, t) = &self.docs[&doc];
        let before = d.transact().state_vector();
        {
            let mut txn = d.transact_mut();
            let len = t.len(&txn);
            let at = at.min(len);
            let delete = delete.min(len - at);
            if delete > 0 {
                t.remove_range(&mut txn, at, delete);
            }
            t.insert(&mut txn, at, insert);
        }
        d.transact().encode_state_as_update_v1(&before)
    }

    /// UTF-16 offsets between characters (editors never split a surrogate pair).
    fn boundaries(&self, doc: DocId) -> Vec<u32> {
        let mut out = vec![0];
        let mut at = 0;
        for c in self.text(doc).chars() {
            at += c.len_utf16() as u32;
            out.push(at);
        }
        out
    }
}

struct Peer {
    guest: Arc<GuestSession>,
    editor: Shared,
}

struct Harness {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    net: MemoryNetwork,
    host: Arc<HostSession>,
    host_editor: Shared,
    backend: Arc<dyn Backend>,
    next_key: u8,
}

impl Harness {
    fn new(permissions: Permissions) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::write(root.join("app/Order.php"), "<?php\n\nclass Order\n{\n}\n").unwrap();
        std::fs::write(root.join("app/Crlf.php"), "a\r\nb\r\n").unwrap();
        std::fs::write(root.join(".env"), "APP_KEY=secret\n").unwrap();
        std::fs::write(root.join("README.md"), "# Shop\n").unwrap();
        let backend: Arc<dyn Backend> = Arc::new(LocalBackend::open(&root).unwrap());
        let net = MemoryNetwork::new();
        let host_editor: Shared = Arc::default();
        let sink_editor = host_editor.clone();
        let host = HostSession::start(
            Arc::new(net.endpoint([1; 32])),
            backend.clone(),
            HostConfig {
                host_name: "Robert".into(),
                machine: Some("laptop".into()),
                permissions,
                excludes: Excludes::defaults(),
                protected: Excludes::protected_defaults(),
                max_file_size: 64 * 1024,
                notice: Some(DEV_PREVIEW_NOTICE.into()),
            },
            Arc::new(DevAutoAccept),
            Arc::new(move |e| {
                let mut ed = sink_editor.lock().unwrap();
                match e {
                    HostEvent::DocUpdate { doc, update } => ed.receive(doc, &update),
                    HostEvent::DocSaved { doc, saved } => {
                        ed.saved.insert(doc, saved);
                    }
                    HostEvent::DocRenamed { doc, path } => ed.renamed.push((doc, path)),
                    HostEvent::DocClosed { doc, reason } => ed.closed.push((doc, reason)),
                    HostEvent::Notice(n) => ed.notices.push(n),
                    HostEvent::Presence {
                        participant,
                        presence,
                    } => {
                        ed.presence.insert(participant, presence);
                    }
                    HostEvent::Awareness { .. } | HostEvent::Changed => {}
                }
            }),
        );
        Self {
            _dir: dir,
            root,
            net,
            host,
            host_editor,
            backend,
            next_key: 2,
        }
    }

    /// The host opens a file in its editor (which shares it).
    async fn host_open(&self, path: &str) -> DocId {
        let o = self.host.open_doc(path, None).await.unwrap();
        self.host_editor.lock().unwrap().open(o.doc, &o.update);
        o.doc
    }

    fn host_edit(&self, doc: DocId, at: u32, delete: u32, insert: &str) {
        let update = self
            .host_editor
            .lock()
            .unwrap()
            .edit(doc, at, delete, insert);
        self.host.host_update(doc, &update).unwrap();
    }

    async fn join(&mut self, name: &str) -> Peer {
        let link = self.host.link();
        self.join_with(name, &link).await.unwrap()
    }

    async fn join_policy(&mut self, name: &str, policy: ReconnectPolicy) -> Peer {
        let link = self.host.link();
        self.join_full(name, &link, policy).await.unwrap()
    }

    /// The endpoint key of the guest that joined last.
    fn last_key(&self) -> [u8; 32] {
        [self.next_key - 1; 32]
    }

    async fn join_with(&mut self, name: &str, link: &SessionLink) -> Result<Peer, String> {
        self.join_full(name, link, ReconnectPolicy::default()).await
    }

    async fn join_full(
        &mut self,
        name: &str,
        link: &SessionLink,
        policy: ReconnectPolicy,
    ) -> Result<Peer, String> {
        let key = [self.next_key; 32];
        self.next_key += 1;
        let editor: Shared = Arc::default();
        let sink_editor = editor.clone();
        let guest = GuestSession::join_with(
            Arc::new(self.net.endpoint(key)),
            link,
            name,
            Arc::new(move |e| {
                let mut ed = sink_editor.lock().unwrap();
                match e {
                    GuestEvent::DocUpdate { doc, update } => ed.receive(doc, &update),
                    GuestEvent::DocSaved { doc, saved } => {
                        ed.saved.insert(doc, saved);
                    }
                    GuestEvent::DocRenamed { doc, path } => ed.renamed.push((doc, path)),
                    GuestEvent::DocClosed { doc, reason } => ed.closed.push((doc, reason)),
                    GuestEvent::Denied(r) => ed.denied.push(r),
                    GuestEvent::Ended(r) => ed.ended = Some(r),
                    GuestEvent::Presence {
                        participant,
                        presence,
                    } => {
                        ed.presence.insert(participant, presence);
                    }
                    GuestEvent::FollowHost(on) => ed.follow_host = on,
                    GuestEvent::Resumed => ed.resumed += 1,
                    GuestEvent::Awareness { .. } | GuestEvent::Changed => {}
                }
            }),
            policy,
        )
        .await?;
        Ok(Peer { guest, editor })
    }
}

impl Peer {
    async fn open(&self, path: &str) -> Result<DocId, String> {
        let o = self.guest.open_doc(path).await?;
        self.editor.lock().unwrap().open(o.doc, &o.update);
        Ok(o.doc)
    }

    fn edit(&self, doc: DocId, at: u32, delete: u32, insert: &str) {
        let update = self.editor.lock().unwrap().edit(doc, at, delete, insert);
        self.guest.update(doc, update);
    }

    fn text(&self, doc: DocId) -> String {
        self.editor.lock().unwrap().text(doc)
    }
}

async fn eventually(what: &str, mut ok: impl FnMut() -> bool) {
    for _ in 0..500 {
        if ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting until {what}");
}

/// A tiny deterministic random generator (xorshift), so failures replay.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }
}

const SNIPPETS: &[&str] = &["a", "ü", "😀", "\n", "fn x() {}", " ", "𝄞", "\r\n"];

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn random_edits_converge_for_two_to_five_peers() {
    for peers in 2..=5u64 {
        let mut h = Harness::new(Permissions::EDIT_FILES);
        h.net.set_latency(Duration::from_millis(peers));
        let doc = h.host_open("app/Order.php").await;
        let mut guests = Vec::new();
        for i in 1..peers {
            let g = h.join(&format!("Guest {i}")).await;
            assert_eq!(g.open("app/Order.php").await.unwrap(), doc);
            guests.push(g);
        }
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ peers);
        for round in 0..150 {
            let who = rng.below(peers);
            let snippet = SNIPPETS[rng.below(SNIPPETS.len() as u64) as usize];
            let editor = if who == 0 {
                h.host_editor.clone()
            } else {
                guests[(who - 1) as usize].editor.clone()
            };
            let b = editor.lock().unwrap().boundaries(doc);
            let start = rng.below(b.len() as u64) as usize;
            let end = if rng.below(4) == 0 {
                (start + rng.below(3) as usize).min(b.len() - 1)
            } else {
                start
            };
            let (at, delete) = (b[start], b[end] - b[start]);
            if who == 0 {
                h.host_edit(doc, at, delete, snippet);
            } else {
                guests[(who - 1) as usize].edit(doc, at, delete, snippet);
            }
            if round % 37 == 0 {
                tokio::time::sleep(Duration::from_millis(3)).await;
            }
        }
        let host_editor = h.host_editor.clone();
        eventually("every replica has the same text", || {
            let host = host_editor.lock().unwrap().text(doc);
            guests.iter().all(|g| g.text(doc) == host)
        })
        .await;
        // The host's authoritative copy matches the replicas, and a save writes it.
        let text = h.host_editor.lock().unwrap().text(doc);
        let saved = h.host.save(doc).await.unwrap();
        assert_eq!(saved.hash, text_hash(&text));
        assert_eq!(
            std::fs::read_to_string(h.root.join("app/Order.php")).unwrap(),
            text
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_saves_and_everyone_agrees_it_is_clean() {
    let mut h = Harness::new(Permissions::EDIT_FILES);
    let doc = h.host_open("app/Crlf.php").await;
    let alex = h.join("Alex").await;
    let sam = h.join("Sam").await;
    alex.open("app/Crlf.php").await.unwrap();
    sam.open("app/Crlf.php").await.unwrap();
    assert_eq!(alex.text(doc), "a\nb\n");
    alex.edit(doc, 1, 0, "!");
    let host_editor = h.host_editor.clone();
    eventually("the edit arrived", || {
        host_editor.lock().unwrap().text(doc) == "a!\nb\n"
    })
    .await;
    alex.guest.save(doc).await.unwrap();
    // The file keeps its CRLF line endings.
    assert_eq!(
        std::fs::read_to_string(h.root.join("app/Crlf.php")).unwrap(),
        "a!\r\nb\r\n"
    );
    let expected = SaveState::of("a!\nb\n");
    for ed in [&alex.editor, &sam.editor, &h.host_editor] {
        let ed = ed.clone();
        let want = expected.clone();
        eventually("the save state arrived", move || {
            ed.lock().unwrap().saved.get(&doc) == Some(&want)
        })
        .await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disk_changes_reach_everyone_and_unsaved_edits_are_kept() {
    let mut h = Harness::new(Permissions::EDIT_FILES);
    let doc = h.host_open("app/Order.php").await;
    let alex = h.join("Alex").await;
    alex.open("app/Order.php").await.unwrap();
    // `git checkout` rewrites the file.
    let new = "<?php\n\nclass Order\n{\n    public int $id;\n}\n";
    std::fs::write(h.root.join("app/Order.php"), new).unwrap();
    h.host
        .on_fs_changed(&FsChanges {
            changed: vec!["app/Order.php".into()],
            removed: vec![],
            git: true,
        })
        .await;
    let host_editor = h.host_editor.clone();
    eventually("the host's editor has the new text", || {
        host_editor.lock().unwrap().text(doc) == new
    })
    .await;
    let ed = alex.editor.clone();
    eventually("the guest has the new text", || {
        ed.lock().unwrap().text(doc) == new
    })
    .await;
    // The save state follows the update in its own message.
    let ed = alex.editor.clone();
    eventually("the guest knows the file is saved", || {
        ed.lock().unwrap().saved.get(&doc) == Some(&SaveState::of(new))
    })
    .await;

    // With unsaved shared edits the memory wins and the host is told.
    alex.edit(doc, 0, 0, "// edited\n");
    eventually("the edit arrived", || {
        host_editor
            .lock()
            .unwrap()
            .text(doc)
            .starts_with("// edited")
    })
    .await;
    std::fs::write(h.root.join("app/Order.php"), "<?php\n").unwrap();
    h.host
        .on_fs_changed(&FsChanges {
            changed: vec!["app/Order.php".into()],
            removed: vec![],
            git: false,
        })
        .await;
    assert!(alex.text(doc).starts_with("// edited"));
    assert!(h
        .host_editor
        .lock()
        .unwrap()
        .notices
        .iter()
        .any(|n| n.contains("changed on disk")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_only_guests_cannot_change_anything() {
    let mut h = Harness::new(Permissions::READ_ONLY);
    let doc = h.host_open("app/Order.php").await;
    let viewer = h.join("Viewer").await;
    let other = h.join("Other").await;
    viewer.open("app/Order.php").await.unwrap();
    other.open("app/Order.php").await.unwrap();
    let before = h.host_editor.lock().unwrap().text(doc);
    viewer.edit(doc, 0, 3, "HACKED");
    assert!(viewer.guest.save(doc).await.is_err());
    let ed = viewer.editor.clone();
    eventually("the guest was told", || {
        !ed.lock().unwrap().denied.is_empty()
    })
    .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(other.text(doc), before);
    assert_eq!(h.host_editor.lock().unwrap().text(doc), before);
    assert_eq!(
        std::fs::read_to_string(h.root.join("app/Order.php")).unwrap(),
        before
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn guests_open_any_visible_file_but_the_shared_list_holds_the_hosts_tabs() {
    let mut h = Harness::new(Permissions::FULL_ACCESS);
    // The host cannot share secrets, Git internals or files outside the project.
    assert!(h.host.open_doc(".env", None).await.is_err());
    assert!(h.host.open_doc(".git/config", None).await.is_err());
    assert!(h.host.open_doc("../outside.txt", None).await.is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc/hostname", h.root.join("link.txt")).unwrap();
        assert!(h.host.open_doc("link.txt", None).await.is_err());
    }
    h.host_open("app/Order.php").await;
    let alex = h.join("Alex").await;
    assert_eq!(
        alex.guest
            .files()
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>(),
        ["app/Order.php"]
    );
    // The project tree is the guest's (S3): any visible file opens, created on demand.
    assert!(alex.open("README.md").await.is_ok());
    assert_eq!(
        alex.guest.files().len(),
        1,
        "only the host's tabs are listed"
    );
    assert!(alex.open(".env").await.is_err());
    assert!(alex.open("../../etc/passwd").await.is_err());
    assert!(alex.open("app/Order.php").await.is_ok());
    // Larger files are not shared.
    std::fs::write(h.root.join("big.txt"), "x".repeat(70 * 1024)).unwrap();
    assert!(h
        .host
        .open_doc("big.txt", None)
        .await
        .unwrap_err()
        .contains("larger"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_the_link_there_is_no_session_and_no_prompt() {
    let mut h = Harness::new(Permissions::EDIT_FILES);
    let mut link = h.host.link();
    link.join_secret[0] ^= 1;
    assert!(h.join_with("Mallory", &link).await.is_err());
    let mut other = h.host.link();
    other.session_id[0] ^= 1;
    assert!(h.join_with("Mallory", &other).await.is_err());
    assert_eq!(h.host.participants().len(), 1);
    assert!(h.host_editor.lock().unwrap().notices.is_empty());
    let alex = h.join("Alex").await;
    assert_eq!(alex.guest.notice().as_deref(), Some(DEV_PREVIEW_NOTICE));
    assert_eq!(h.host.participants().len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn renames_and_deletes_follow_the_document() {
    let mut h = Harness::new(Permissions::EDIT_FILES);
    let doc = h.host_open("app/Order.php").await;
    let alex = h.join("Alex").await;
    alex.open("app/Order.php").await.unwrap();
    alex.edit(doc, 0, 0, "x");
    std::fs::rename(h.root.join("app"), h.root.join("src")).unwrap();
    h.host.on_renamed("app", "src");
    let ed = alex.editor.clone();
    eventually("the guest saw the rename", || {
        ed.lock().unwrap().renamed == [(doc, "src/Order.php".to_string())]
    })
    .await;
    assert_eq!(h.host.shared_files()[0].path, "src/Order.php");
    // The edit made before the rename is saved under the new name.
    h.host.save(doc).await.unwrap();
    assert!(std::fs::read_to_string(h.root.join("src/Order.php"))
        .unwrap()
        .starts_with('x'));
    h.backend.delete("src/Order.php").unwrap();
    h.host.on_deleted("src/Order.php");
    eventually("the guest saw the delete", || {
        ed.lock()
            .unwrap()
            .closed
            .iter()
            .any(|(d, r)| *d == doc && r == "Order.php was deleted by Robert")
    })
    .await;
    assert!(h.host.shared_files().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn leaving_disconnecting_and_ending() {
    let mut h = Harness::new(Permissions::EDIT_FILES);
    let doc = h.host_open("app/Order.php").await;
    let alex = h.join("Alex").await;
    let sam = h.join("Sam").await;
    alex.open("app/Order.php").await.unwrap();
    sam.open("app/Order.php").await.unwrap();
    assert_eq!(
        h.host
            .participants()
            .iter()
            .map(|p| p.color)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    alex.guest.leave().await;
    let host = h.host.clone();
    eventually("Alex left", || host.participants().len() == 2).await;
    let host_editor = h.host_editor.clone();
    eventually("the host was told", || {
        host_editor
            .lock()
            .unwrap()
            .notices
            .iter()
            .any(|n| n == "Alex left the session.")
    })
    .await;
    // Without a grace period a dropped connection counts as leaving; the others keep
    // editing. (Reconnecting is tested below.)
    h.host.set_reconnect_grace(Duration::ZERO);
    let joe = h.join_policy("Joe", fast(5_000)).await;
    joe.open("app/Order.php").await.unwrap();
    h.net.disconnect([4; 32]);
    eventually("Joe's connection is gone", || {
        host.participants().len() == 2
    })
    .await;
    let jed = joe.editor.clone();
    eventually("Joe was told", || jed.lock().unwrap().ended.is_some()).await;
    sam.edit(doc, 0, 0, "still here ");
    eventually("Sam's edit arrived", || {
        host_editor
            .lock()
            .unwrap()
            .text(doc)
            .starts_with("still here")
    })
    .await;
    h.host.end("The host ended the session.").await;
    let sed = sam.editor.clone();
    eventually("Sam was told", || {
        sed.lock().unwrap().ended.as_deref() == Some("The host ended the session.")
    })
    .await;
    assert!(sam.guest.open_doc("app/Order.php").await.is_err());
}

fn presence_in(file: &str, line: u32) -> Presence {
    Presence {
        file: Some(file.into()),
        caret: Some(Caret { line, column: 1 }),
        anchor: None,
        viewport: Some(Viewport {
            top_line: line.saturating_sub(10).max(1),
            bottom_line: line + 20,
        }),
        following: None,
    }
}

fn seen(editor: &Shared, who: ParticipantId) -> Option<Presence> {
    editor.lock().unwrap().presence.get(&who).cloned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn presence_reaches_everyone_through_the_host() {
    let mut h = Harness::new(Permissions::EDIT_FILES);
    let alex = h.join("Alex").await;
    let sam = h.join("Sam").await;
    let (alex_id, sam_id) = (alex.guest.me(), sam.guest.me());

    // The host's presence reaches both guests.
    h.host.host_presence(presence_in("app/Order.php", 3));
    for g in [&alex, &sam] {
        let ed = g.editor.clone();
        eventually("the host's presence arrived", || {
            seen(&ed, ParticipantId::HOST) == Some(presence_in("app/Order.php", 3))
        })
        .await;
    }

    // A guest's presence reaches the host's UI and the other guest, and following the host
    // shows up as well.
    let mut follows = presence_in("app/Order.php", 3);
    follows.following = Some(ParticipantId::HOST);
    alex.guest.set_presence(follows.clone());
    let host_ed = h.host_editor.clone();
    eventually("the host sees Alex", || {
        seen(&host_ed, alex_id) == Some(follows.clone())
    })
    .await;
    let sam_ed = sam.editor.clone();
    eventually("Sam sees Alex", || {
        seen(&sam_ed, alex_id) == Some(follows.clone())
    })
    .await;
    assert_eq!(h.host.presence().len(), 2);

    // A guest that joins later gets everyone's presence at once.
    let joe = h.join("Joe").await;
    let joe_ed = joe.editor.clone();
    eventually("Joe got the snapshot", || {
        seen(&joe_ed, ParticipantId::HOST).is_some() && seen(&joe_ed, alex_id).is_some()
    })
    .await;
    assert_eq!(seen(&joe_ed, sam_id), None, "Sam sent nothing yet");

    // Following someone who is not in the session, or oneself, is dropped by the host.
    let mut bogus = presence_in("README.md", 1);
    bogus.following = Some(ParticipantId(99));
    sam.guest.set_presence(bogus);
    eventually("Sam's presence arrived", || {
        seen(&host_ed, sam_id).is_some_and(|p| p.file.as_deref() == Some("README.md"))
    })
    .await;
    assert_eq!(seen(&host_ed, sam_id).unwrap().following, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn guests_never_learn_files_they_cannot_see() {
    let mut h = Harness::new(Permissions::READ_ONLY);
    let alex = h.join("Alex").await;
    let sam = h.join("Sam").await;
    let alex_ed = alex.editor.clone();

    // The host looks at .env: guests see that the host is somewhere, not where.
    h.host.host_presence(presence_in(".env", 1));
    eventually("the host's presence arrived", || {
        seen(&alex_ed, ParticipantId::HOST).is_some()
    })
    .await;
    assert_eq!(
        seen(&alex_ed, ParticipantId::HOST),
        Some(Presence::default())
    );
    // The host itself keeps the real presence.
    assert_eq!(h.host.presence()[0].1.file.as_deref(), Some(".env"));

    // A guest that names a hidden or absolute path passes nothing on.
    sam.guest.set_presence(presence_in(".git/config", 2));
    let sam_id = sam.guest.me();
    eventually("Sam's presence arrived", || {
        seen(&alex_ed, sam_id).is_some()
    })
    .await;
    assert_eq!(seen(&alex_ed, sam_id), Some(Presence::default()));
    sam.guest.set_presence(presence_in("/etc/passwd", 2));
    let host_ed = h.host_editor.clone();
    eventually("the host saw it", || {
        seen(&host_ed, sam_id) == Some(Presence::default())
    })
    .await;

    // A visible file comes through with the position.
    h.host.host_presence(presence_in("app/Order.php", 4));
    eventually("the host moved", || {
        seen(&alex_ed, ParticipantId::HOST) == Some(presence_in("app/Order.php", 4))
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn make_everyone_follow_you_reaches_every_guest() {
    let mut h = Harness::new(Permissions::READ_ONLY);
    let alex = h.join("Alex").await;
    assert!(!h.host.follow_everyone());
    h.host.set_follow_everyone(true);
    assert!(h.host.follow_everyone());
    let alex_ed = alex.editor.clone();
    eventually("Alex was asked", || alex_ed.lock().unwrap().follow_host).await;
    assert!(alex.guest.follow_host());
    // A guest that joins while it is on is asked at once.
    let sam = h.join("Sam").await;
    let sam_ed = sam.editor.clone();
    eventually("Sam was asked", || sam_ed.lock().unwrap().follow_host).await;
    h.host.set_follow_everyone(false);
    eventually("Alex was released", || !alex_ed.lock().unwrap().follow_host).await;
    eventually("Sam was released", || !sam_ed.lock().unwrap().follow_host).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn presence_follows_permission_changes_and_leaving() {
    let mut h = Harness::new(Permissions::READ_ONLY);
    let alex = h.join("Alex").await;
    let sam = h.join("Sam").await;
    let (alex_id, sam_id) = (alex.guest.me(), sam.guest.me());
    sam.guest.set_presence(presence_in("README.md", 1));
    let alex_ed = alex.editor.clone();
    eventually("Alex sees Sam", || seen(&alex_ed, sam_id).is_some()).await;

    // Leaving removes the presence everywhere.
    sam.guest.leave().await;
    let host = h.host.clone();
    eventually("Sam left", || host.participants().len() == 2).await;
    eventually("Alex forgot Sam", || {
        alex.guest.presence().iter().all(|(id, _)| *id != sam_id)
    })
    .await;
    assert!(h.host.presence().iter().all(|(id, _)| *id != sam_id));

    // A permission change sends everyone's presence again under the new rules.
    h.host.host_presence(presence_in("app/Order.php", 2));
    eventually("Alex sees the host", || {
        seen(&alex_ed, ParticipantId::HOST).is_some()
    })
    .await;
    alex_ed.lock().unwrap().presence.clear();
    assert!(h
        .host
        .set_guest_permissions(alex_id, Permissions::EDIT_FILES));
    eventually("Alex got the presence again", || {
        seen(&alex_ed, ParticipantId::HOST) == Some(presence_in("app/Order.php", 2))
    })
    .await;
}

// ---- reconnect --------------------------------------------------------------------------

/// Quick retries for tests: every 20 ms, for `give_up_ms`.
fn fast(give_up_ms: u64) -> ReconnectPolicy {
    ReconnectPolicy {
        backoff: vec![Duration::from_millis(20)],
        give_up_after: Duration::from_millis(give_up_ms),
    }
}

impl Peer {
    /// What the UI does on `Resumed` for one document: Yjs sync step 1 and 2 both ways.
    async fn resync(&self, doc: DocId) -> Result<(), String> {
        let sv = {
            let ed = self.editor.lock().unwrap();
            let (d, _) = &ed.docs[&doc];
            let sv = d.transact().state_vector().encode_v1();
            sv
        };
        let synced = self.guest.sync_doc(doc, sv).await?;
        let missing = {
            let ed = self.editor.lock().unwrap();
            let (d, _) = &ed.docs[&doc];
            d.transact_mut()
                .apply_update(Update::decode_v1(&synced.update).unwrap())
                .unwrap();
            let host_sv = StateVector::decode_v1(&synced.state_vector).unwrap();
            let missing = d.transact().encode_state_as_update_v1(&host_sv);
            missing
        };
        self.guest.update(doc, missing);
        Ok(())
    }

    fn resumed(&self) -> usize {
        self.editor.lock().unwrap().resumed
    }
}

fn reconnecting(host: &HostSession, name: &str) -> bool {
    host.participants()
        .iter()
        .any(|p| p.name == name && p.reconnecting)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_reconnects_and_keeps_its_place_and_its_unsent_edits() {
    let mut h = Harness::new(Permissions::EDIT_FILES);
    let doc = h.host_open("app/Order.php").await;
    let alex = h.join_policy("Alex", fast(5_000)).await;
    let key = h.last_key();
    alex.open("app/Order.php").await.unwrap();
    alex.guest.set_presence(presence_in("app/Order.php", 3));
    let me = alex.guest.me();
    let color = h.host.participants()[1].color;

    h.net.set_offline(key, true);
    let host = h.host.clone();
    eventually("the host sees Alex reconnecting", || {
        reconnecting(&host, "Alex")
    })
    .await;
    let g = alex.guest.clone();
    eventually("Alex is reconnecting", || !g.connection().is_connected()).await;
    assert_eq!(h.host.participants().len(), 2, "the place is kept");
    // Requests fail fast while the connection is down; edits stay in the editor.
    assert_eq!(
        alex.guest.open_doc("README.md").await.unwrap_err(),
        forge_collab::reconnect::RECONNECTING
    );
    alex.edit(doc, 0, 0, "// offline\n");
    h.host_edit(doc, 5, 0, " // host");

    h.net.set_offline(key, false);
    eventually("Alex is back", || {
        g.connection().is_connected() && !reconnecting(&host, "Alex")
    })
    .await;
    eventually("Alex was told to resync", || alex.resumed() == 1).await;
    assert_eq!(alex.guest.me(), me);
    assert_eq!(h.host.participants()[1].color, color);
    alex.resync(doc).await.unwrap();
    let host_editor = h.host_editor.clone();
    eventually("both edits are everywhere", || {
        let text = host_editor.lock().unwrap().text(doc);
        text.contains("// offline") && text.contains("// host") && alex.text(doc) == text
    })
    .await;
    // Presence came again, and Alex keeps editing as before.
    eventually("the host knows where Alex is", || {
        host.presence()
            .iter()
            .any(|(id, p)| *id == me && p.caret.is_some())
    })
    .await;
    alex.edit(doc, 0, 0, "x");
    eventually("live edits flow again", || {
        host_editor
            .lock()
            .unwrap()
            .text(doc)
            .starts_with("x// offline")
    })
    .await;
    assert!(alex.editor.lock().unwrap().ended.is_none());
    let lines = h.host.session_hex();
    assert!(!lines.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_host_frees_the_place_after_the_grace_period() {
    let mut h = Harness::new(Permissions::EDIT_FILES);
    h.host.set_reconnect_grace(Duration::from_millis(150));
    let alex = h.join_policy("Alex", fast(5_000)).await;
    let key = h.last_key();
    h.net.set_offline(key, true);
    let host = h.host.clone();
    eventually("Alex's place is gone", || host.participants().len() == 1).await;
    let host_editor = h.host_editor.clone();
    eventually("the host was told", || {
        host_editor
            .lock()
            .unwrap()
            .notices
            .iter()
            .any(|n| n == "Alex lost the connection and did not come back.")
    })
    .await;
    h.net.set_offline(key, false);
    let ed = alex.editor.clone();
    eventually("Alex hears that the place is gone", || {
        ed.lock()
            .unwrap()
            .ended
            .as_deref()
            .is_some_and(|r| r.contains("no longer keeps your place"))
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_gives_up_when_the_host_stays_unreachable() {
    let mut h = Harness::new(Permissions::READ_ONLY);
    let alex = h.join_policy("Alex", fast(200)).await;
    h.net.set_offline([1; 32], true);
    let ed = alex.editor.clone();
    eventually("Alex gave up", || {
        ed.lock().unwrap().ended.as_deref() == Some(forge_collab::reconnect::GAVE_UP)
    })
    .await;
    assert!(alex.guest.is_ended());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_removed_while_away_learns_why() {
    let mut h = Harness::new(Permissions::READ_ONLY);
    let alex = h.join_policy("Alex", fast(5_000)).await;
    let key = h.last_key();
    h.net.set_offline(key, true);
    let host = h.host.clone();
    eventually("Alex is away", || reconnecting(&host, "Alex")).await;
    assert!(h.host.remove(alex.guest.me()));
    h.net.set_offline(key, false);
    let ed = alex.editor.clone();
    eventually("Alex was told", || {
        ed.lock().unwrap().ended.as_deref() == Some("The host removed you from the session.")
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nobody_else_can_take_a_place_back() {
    use forge_collab::protocol::{
        GuestMessage, HostMessage, MAX_GUEST_FRAME, MAX_HOST_FRAME, PROTOCOL_VERSION,
    };
    use forge_collab::transport::{read_msg, write_msg, Transport};
    let mut h = Harness::new(Permissions::READ_ONLY);
    let alex = h.join_policy("Alex", fast(5_000)).await;
    let key = h.last_key();
    h.net.set_offline(key, true);
    let host = h.host.clone();
    eventually("Alex is away", || reconnecting(&host, "Alex")).await;
    // Another endpoint (or a guessed token) names Alex's place.
    let mallory = h.net.endpoint([99; 32]);
    let mut conn = mallory.connect([1; 32], vec![]).await.unwrap();
    let resume = GuestMessage::Resume {
        protocol: PROTOCOL_VERSION,
        session: serde_bytes::ByteBuf::from(h.host.link().session_id.to_vec()),
        participant: alex.guest.me(),
        token: serde_bytes::ByteBuf::from(vec![0; 32]),
    };
    write_msg(&mut conn.send, &resume, MAX_GUEST_FRAME)
        .await
        .unwrap();
    let answer: Option<HostMessage> = read_msg(&mut conn.recv, MAX_HOST_FRAME).await.unwrap();
    assert!(
        matches!(answer, Some(HostMessage::Refused { .. })),
        "{answer:?}"
    );
    assert!(reconnecting(&h.host, "Alex"), "the place is still Alex's");
    h.net.set_offline(key, false);
    let g = alex.guest.clone();
    eventually("Alex is back", || g.connection().is_connected()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn documents_closed_while_away_fail_to_resync() {
    let mut h = Harness::new(Permissions::EDIT_FILES);
    let doc = h.host_open("README.md").await;
    let alex = h.join_policy("Alex", fast(5_000)).await;
    let key = h.last_key();
    alex.open("README.md").await.unwrap();
    h.net.set_offline(key, true);
    let host = h.host.clone();
    eventually("Alex is away", || reconnecting(&host, "Alex")).await;
    std::fs::remove_file(h.root.join("README.md")).unwrap();
    h.host.on_deleted("README.md");
    h.net.set_offline(key, false);
    let g = alex.guest.clone();
    eventually("Alex is back", || g.connection().is_connected()).await;
    let err = alex.resync(doc).await.unwrap_err();
    assert_eq!(err, "The file is no longer shared.");
}
