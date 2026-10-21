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
use forge_collab::protocol::{text_hash, DocId, SaveState};
use forge_collab::transport::memory::MemoryNetwork;
use forge_core::FsChanges;
use yrs::updates::decoder::Decode;
use yrs::{Doc, GetString, OffsetKind, Options, ReadTxn, Text, TextRef, Transact, Update};

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

    async fn join_with(&mut self, name: &str, link: &SessionLink) -> Result<Peer, String> {
        let key = [self.next_key; 32];
        self.next_key += 1;
        let editor: Shared = Arc::default();
        let sink_editor = editor.clone();
        let guest = GuestSession::join(
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
                    GuestEvent::Awareness { .. } | GuestEvent::Changed => {}
                }
            }),
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
    assert_eq!(
        alex.editor.lock().unwrap().saved.get(&doc),
        Some(&SaveState::of(new))
    );

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
async fn only_files_the_host_shares_can_be_opened() {
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
    assert!(
        alex.open("README.md").await.is_err(),
        "not open on the host"
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
    // A dropped connection counts as leaving; the others keep editing.
    let joe = h.join("Joe").await;
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
