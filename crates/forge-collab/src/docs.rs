//! The host's authoritative copy of each shared file: a `yrs` document with one text named
//! `content`, created with UTF-16 offsets so positions match JavaScript and CodeMirror.
//!
//! Every participant's editor holds a Yjs replica of it; the host applies their updates here
//! (after the policy check), serves late joiners the whole state, writes the text to disk
//! and applies changes from disk as minimal edits.

use std::collections::{BTreeSet, HashMap};

use serde_json::{json, Value};
use yrs::sync::awareness::{AwarenessUpdate, AwarenessUpdateEntry};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{
    ClientID, Doc, GetString, OffsetKind, Options, ReadTxn, StateVector, Text, TextRef, Transact,
    Update,
};

use crate::presence::{caret_color, selection_color, Participant, ParticipantId};
use crate::protocol::{DocId, SaveState};
use crate::textdiff;

/// Name of the shared text in every document (`ydoc.getText("content")` in the UI).
pub const TEXT_NAME: &str = "content";

/// Transaction origin of changes read from disk; no participant's undo manager tracks it.
pub const DISK_ORIGIN: &str = "disk";

/// Line ending of a file on disk. Documents always hold `\n`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eol {
    Lf,
    CrLf,
}

/// Split a file's text into `\n`-only text and its line ending (the UI's `normalizeEol`).
pub fn normalize_eol(raw: &str) -> (String, Eol) {
    let crlf = raw.matches("\r\n").count();
    let lf = raw.matches('\n').count() - crlf;
    let text = raw.replace("\r\n", "\n").replace('\r', "\n");
    (text, if crlf > lf { Eol::CrLf } else { Eol::Lf })
}

pub fn with_eol(text: &str, eol: Eol) -> String {
    match eol {
        Eol::Lf => text.to_string(),
        Eol::CrLf => text.replace('\n', "\r\n"),
    }
}

/// What [`SharedDoc::apply_disk`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiskChange {
    /// The disk already has this text (e.g. the host's own save), or it changed back to the
    /// saved text while the document has unsaved changes (an open conflict is gone).
    Unchanged,
    /// Applied; the update to send to everyone.
    Applied(Vec<u8>),
    /// The disk now holds the document's text: it is saved, there is nothing to apply.
    Matches,
    /// The document has unsaved changes and the file changed on disk: PHPStorm's File Cache
    /// Conflict, open until [`SharedDoc::resolve_conflict`] (or a save, or the texts agree).
    Conflict,
}

/// The host's answer to a File Cache Conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// Load File System Changes: the document takes the disk's text (minimal edits).
    LoadFileSystem,
    /// Keep Memory Changes: the document keeps its text and stays unsaved; saving it
    /// overwrites the file.
    KeepMemory,
}

/// One shared file.
pub struct SharedDoc {
    pub id: DocId,
    pub path: String,
    doc: Doc,
    text: TextRef,
    /// The text as on disk (`\n` line endings).
    saved: String,
    pub eol: Eol,
    /// Participants with the document open (the host's editor is [`ParticipantId::HOST`]).
    pub open_by: BTreeSet<ParticipantId>,
    /// Awareness client ids and who owns them, with the last clock seen.
    clients: HashMap<u64, (ParticipantId, u32)>,
    /// The file's text on disk (`\n` line endings) while a File Cache Conflict is open.
    conflict: Option<(String, Eol)>,
}

impl SharedDoc {
    /// A document holding `initial` (the host's editor text, or the file), saved as `disk`.
    pub fn new(id: DocId, path: String, disk_raw: &str, initial: Option<&str>) -> Self {
        let (saved, eol) = normalize_eol(disk_raw);
        let doc = Doc::with_options(Options {
            offset_kind: OffsetKind::Utf16,
            ..Options::default()
        });
        let text = doc.get_or_insert_text(TEXT_NAME);
        {
            let mut txn = doc.transact_mut_with(DISK_ORIGIN);
            let content = initial
                .map(|t| normalize_eol(t).0)
                .unwrap_or_else(|| saved.clone());
            text.insert(&mut txn, 0, &content);
        }
        Self {
            id,
            path,
            doc,
            text,
            saved,
            eol,
            open_by: BTreeSet::new(),
            clients: HashMap::new(),
            conflict: None,
        }
    }

    /// The whole document as one update (for participants opening it).
    pub fn full_update(&self) -> Vec<u8> {
        self.doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default())
    }

    /// What a replica with state vector `sv` (Yjs v1 encoding) is missing, and this
    /// document's state vector (Yjs sync step 2 and step 1, for a reconnect).
    pub fn diff_since(&self, sv: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
        let sv = StateVector::decode_v1(sv).map_err(|e| format!("invalid state vector: {e}"))?;
        let txn = self.doc.transact();
        Ok((
            txn.encode_state_as_update_v1(&sv),
            txn.state_vector().encode_v1(),
        ))
    }

    pub fn text(&self) -> String {
        self.text.get_string(&self.doc.transact())
    }

    pub fn saved_text(&self) -> &str {
        &self.saved
    }

    pub fn save_state(&self) -> SaveState {
        SaveState::of(&self.saved)
    }

    pub fn is_dirty(&self) -> bool {
        self.text() != self.saved
    }

    /// Integrate a participant's update.
    pub fn apply_update(&mut self, bytes: &[u8]) -> Result<(), String> {
        let update = Update::decode_v1(bytes).map_err(|e| format!("invalid update: {e}"))?;
        self.doc
            .transact_mut()
            .apply_update(update)
            .map_err(|e| format!("invalid update: {e}"))
    }

    /// The text was written to disk (this also settles an open conflict: memory won).
    pub fn mark_saved(&mut self, text: String) {
        self.saved = text;
        self.conflict = None;
    }

    /// Whether a File Cache Conflict is open (the host has to choose).
    pub fn has_conflict(&self) -> bool {
        self.conflict.is_some()
    }

    /// The file changed on disk. Without unsaved changes the difference is applied as
    /// minimal edits in one transaction (origin [`DISK_ORIGIN`]); with unsaved changes the
    /// document is kept and a conflict opens (a later disk change replaces its disk text).
    pub fn apply_disk(&mut self, disk_raw: &str) -> DiskChange {
        let (disk, eol) = normalize_eol(disk_raw);
        let current = self.text();
        if disk == current {
            if disk == self.saved && self.conflict.is_none() {
                return DiskChange::Unchanged;
            }
            self.eol = eol;
            self.mark_saved(disk);
            return DiskChange::Matches;
        }
        if disk == self.saved {
            // Changed back (e.g. `git stash` and `git stash pop`): the unsaved changes are
            // based on the disk's text again.
            self.conflict = None;
            return DiskChange::Unchanged;
        }
        if current != self.saved {
            self.conflict = Some((disk, eol));
            return DiskChange::Conflict;
        }
        self.eol = eol;
        self.saved = disk.clone();
        DiskChange::Applied(self.replace_text(&current, &disk))
    }

    /// Answer an open conflict. Returns the update to send to everyone when the document
    /// took the disk's text (`None` when nothing changed); the save state changes in both
    /// cases (the saved text is the disk's now).
    pub fn resolve_conflict(&mut self, how: Resolution) -> Result<Option<Vec<u8>>, String> {
        let (disk, eol) = self
            .conflict
            .take()
            .ok_or("The file has no conflict to resolve.")?;
        self.eol = eol;
        self.saved = disk.clone();
        match how {
            Resolution::KeepMemory => Ok(None),
            Resolution::LoadFileSystem => {
                let current = self.text();
                let update = self.replace_text(&current, &disk);
                Ok((current != disk).then_some(update))
            }
        }
    }

    /// After an edit: an open conflict is gone when the document now has the disk's text.
    /// True when it was settled (the document is saved now).
    pub fn settle_conflict(&mut self) -> bool {
        let Some((disk, eol)) = &self.conflict else {
            return false;
        };
        if self.text() != *disk {
            return false;
        }
        let (disk, eol) = (disk.clone(), *eol);
        self.eol = eol;
        self.mark_saved(disk);
        true
    }

    /// Turn `current` (the document's text) into `target` with minimal edits in one
    /// transaction with origin [`DISK_ORIGIN`]; the update to send.
    fn replace_text(&mut self, current: &str, target: &str) -> Vec<u8> {
        let before = self.doc.transact().state_vector();
        let edits = textdiff::edits(current, target);
        {
            let mut txn = self.doc.transact_mut_with(DISK_ORIGIN);
            for e in edits.iter().rev() {
                if e.delete > 0 {
                    self.text
                        .remove_range(&mut txn, e.at as u32, e.delete as u32);
                }
                if !e.insert.is_empty() {
                    self.text.insert(&mut txn, e.at as u32, &e.insert);
                }
            }
        }
        self.doc.transact().encode_state_as_update_v1(&before)
    }

    /// Check and rewrite a participant's awareness update: entries for client ids that
    /// belong to someone else are dropped, and the name and color are the host's, so nobody
    /// can show a caret in another participant's name. `None` when nothing is left.
    pub fn stamp_awareness(&mut self, from: &Participant, bytes: &[u8]) -> Option<Vec<u8>> {
        let update = AwarenessUpdate::decode_v1(bytes).ok()?;
        let mut out = AwarenessUpdate {
            clients: Default::default(),
        };
        for (client, entry) in update.clients {
            let key = client.get();
            match self.clients.get(&key) {
                Some((owner, _)) if *owner != from.id => continue,
                _ => {}
            }
            let json = if entry.json.as_ref() == "null" {
                "null".to_string()
            } else {
                let Ok(Value::Object(mut state)) = serde_json::from_str::<Value>(&entry.json)
                else {
                    continue;
                };
                state.insert(
                    "user".into(),
                    json!({
                        "name": from.name,
                        "color": caret_color(from.color),
                        "colorLight": selection_color(from.color),
                        "colorIndex": from.color,
                        "participant": from.id,
                    }),
                );
                Value::Object(state).to_string()
            };
            self.clients.insert(key, (from.id, entry.clock));
            out.clients.insert(
                client,
                AwarenessUpdateEntry {
                    clock: entry.clock,
                    json: json.into(),
                },
            );
        }
        (!out.clients.is_empty()).then(|| out.encode_v1())
    }

    /// Remove a participant's carets (they closed the file or left); the update to send.
    pub fn remove_participant(&mut self, who: ParticipantId) -> Option<Vec<u8>> {
        self.open_by.remove(&who);
        let mut out = AwarenessUpdate {
            clients: Default::default(),
        };
        self.clients.retain(|client, (owner, clock)| {
            if *owner != who {
                return true;
            }
            out.clients.insert(
                ClientID::new(*client),
                AwarenessUpdateEntry {
                    clock: *clock + 1,
                    json: "null".into(),
                },
            );
            false
        });
        (!out.clients.is_empty()).then(|| out.encode_v1())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::Permissions;
    use crate::presence::Role;

    fn replica(update: &[u8]) -> (Doc, TextRef) {
        let doc = Doc::with_options(Options {
            offset_kind: OffsetKind::Utf16,
            ..Options::default()
        });
        let text = doc.get_or_insert_text(TEXT_NAME);
        doc.transact_mut()
            .apply_update(Update::decode_v1(update).unwrap())
            .unwrap();
        (doc, text)
    }

    fn participant(id: u32, name: &str, color: u8) -> Participant {
        Participant {
            id: ParticipantId(id),
            name: name.into(),
            role: Role::Guest,
            color,
            permissions: Permissions::EDIT_FILES,
            reconnecting: false,
        }
    }

    #[test]
    fn line_endings_are_normalized_and_kept() {
        assert_eq!(normalize_eol("a\r\nb\r\n"), ("a\nb\n".into(), Eol::CrLf));
        assert_eq!(normalize_eol("a\nb\r\nc\n"), ("a\nb\nc\n".into(), Eol::Lf));
        assert_eq!(with_eol("a\nb", Eol::CrLf), "a\r\nb");
        let d = SharedDoc::new(DocId(1), "a.php".into(), "x\r\ny\r\n", None);
        assert_eq!(d.text(), "x\ny\n");
        assert_eq!(d.eol, Eol::CrLf);
        assert!(!d.is_dirty());
    }

    #[test]
    fn the_hosts_buffer_wins_and_is_dirty() {
        let d = SharedDoc::new(DocId(1), "a.php".into(), "disk\n", Some("edited\n"));
        assert_eq!(d.text(), "edited\n");
        assert!(d.is_dirty());
        assert_eq!(d.save_state(), SaveState::of("disk\n"));
    }

    #[test]
    fn replicas_converge_with_the_host() {
        let mut host = SharedDoc::new(DocId(1), "a.php".into(), "hello world\n", None);
        let (guest, text) = replica(&host.full_update());
        let before = guest.transact().state_vector();
        {
            let mut txn = guest.transact_mut();
            text.insert(&mut txn, 5, ",");
        }
        let update = guest.transact().encode_state_as_update_v1(&before);
        host.apply_update(&update).unwrap();
        assert_eq!(host.text(), "hello, world\n");
        assert!(host.apply_update(&[1, 2, 3]).is_err());
    }

    #[test]
    fn disk_changes_apply_as_small_edits_unless_dirty() {
        let mut host = SharedDoc::new(DocId(1), "a.php".into(), "a😀\nb\n", None);
        let (guest, text) = replica(&host.full_update());
        let DiskChange::Applied(update) = host.apply_disk("a😀\nB\nc\n") else {
            panic!()
        };
        assert_eq!(host.text(), "a😀\nB\nc\n");
        assert!(!host.is_dirty());
        guest
            .transact_mut()
            .apply_update(Update::decode_v1(&update).unwrap())
            .unwrap();
        assert_eq!(text.get_string(&guest.transact()), "a😀\nB\nc\n");
        // The host's own save echoes back: nothing to do.
        assert_eq!(host.apply_disk("a😀\nB\nc\n"), DiskChange::Unchanged);
        // Unsaved edits are kept.
        let mut dirty = SharedDoc::new(DocId(2), "b.php".into(), "x\n", Some("y\n"));
        assert_eq!(dirty.apply_disk("z\n"), DiskChange::Conflict);
        assert_eq!(dirty.text(), "y\n");
    }

    #[test]
    fn a_file_cache_conflict_waits_for_the_host() {
        let mut d = SharedDoc::new(DocId(1), "a.php".into(), "one\n", Some("one\nmemory\n"));
        assert!(!d.has_conflict());
        assert_eq!(d.apply_disk("one\r\ndisk\r\n"), DiskChange::Conflict);
        assert!(d.has_conflict());
        // A second change on disk replaces the disk side; memory is untouched.
        assert_eq!(d.apply_disk("one\r\ndisk 2\r\n"), DiskChange::Conflict);
        assert_eq!(d.text(), "one\nmemory\n");
        let (guest, text) = replica(&d.full_update());
        let update = d
            .resolve_conflict(Resolution::LoadFileSystem)
            .unwrap()
            .expect("the text changed");
        guest
            .transact_mut()
            .apply_update(Update::decode_v1(&update).unwrap())
            .unwrap();
        assert_eq!(text.get_string(&guest.transact()), "one\ndisk 2\n");
        assert!(!d.is_dirty() && !d.has_conflict());
        assert_eq!(d.eol, Eol::CrLf);
        assert!(d.resolve_conflict(Resolution::KeepMemory).is_err());
    }

    #[test]
    fn keeping_memory_changes_leaves_the_document_unsaved() {
        let mut d = SharedDoc::new(DocId(1), "a.php".into(), "x\n", Some("memory\n"));
        assert_eq!(d.apply_disk("disk\n"), DiskChange::Conflict);
        assert_eq!(d.resolve_conflict(Resolution::KeepMemory), Ok(None));
        assert_eq!(d.text(), "memory\n");
        assert_eq!(d.saved_text(), "disk\n");
        assert!(d.is_dirty() && !d.has_conflict());
        assert_eq!(d.save_state(), SaveState::of("disk\n"));
    }

    #[test]
    fn a_conflict_goes_away_when_the_texts_agree() {
        // The disk changes back to the saved text.
        let mut d = SharedDoc::new(DocId(1), "a.php".into(), "x\n", Some("memory\n"));
        assert_eq!(d.apply_disk("disk\n"), DiskChange::Conflict);
        assert_eq!(d.apply_disk("x\n"), DiskChange::Unchanged);
        assert!(!d.has_conflict() && d.is_dirty());
        // The disk gets the document's text (someone saved the same text elsewhere).
        assert_eq!(d.apply_disk("disk\n"), DiskChange::Conflict);
        assert_eq!(d.apply_disk("memory\n"), DiskChange::Matches);
        assert!(!d.has_conflict() && !d.is_dirty());
        // A participant edits the document into the disk's text.
        let mut d = SharedDoc::new(DocId(2), "b.php".into(), "x\n", Some("memory\n"));
        assert_eq!(d.apply_disk("disk\n"), DiskChange::Conflict);
        assert!(!d.settle_conflict());
        {
            let mut txn = d.doc.transact_mut();
            d.text.remove_range(&mut txn, 0, 6);
            d.text.insert(&mut txn, 0, "disk");
        }
        assert!(d.settle_conflict());
        assert!(!d.has_conflict() && !d.is_dirty());
        // Saving settles it as well (memory wins).
        let mut d = SharedDoc::new(DocId(3), "c.php".into(), "x\n", Some("memory\n"));
        assert_eq!(d.apply_disk("disk\n"), DiskChange::Conflict);
        d.mark_saved("memory\n".into());
        assert!(!d.has_conflict() && !d.is_dirty());
    }

    fn awareness(client: u64, clock: u32, json: &str) -> Vec<u8> {
        let mut u = AwarenessUpdate {
            clients: Default::default(),
        };
        u.clients.insert(
            ClientID::new(client),
            AwarenessUpdateEntry {
                clock,
                json: json.into(),
            },
        );
        u.encode_v1()
    }

    fn decode(bytes: &[u8]) -> HashMap<u64, (u32, String)> {
        AwarenessUpdate::decode_v1(bytes)
            .unwrap()
            .clients
            .into_iter()
            .map(|(c, e)| (c.get(), (e.clock, e.json.to_string())))
            .collect()
    }

    #[test]
    fn awareness_is_stamped_and_cannot_be_spoofed() {
        let mut d = SharedDoc::new(DocId(1), "a.php".into(), "", None);
        let alex = participant(2, "Alex", 2);
        let eve = participant(3, "Eve", 3);
        let forged = r#"{"user":{"name":"Robert","color":"red"},"cursor":{"anchor":1}}"#;
        let out = decode(&d.stamp_awareness(&alex, &awareness(77, 1, forged)).unwrap());
        let state: Value = serde_json::from_str(&out[&77].1).unwrap();
        assert_eq!(state["user"]["name"], "Alex");
        assert_eq!(state["user"]["color"], "var(--collab-2)");
        assert_eq!(state["cursor"]["anchor"], 1);
        // Eve cannot move Alex's caret.
        assert!(d.stamp_awareness(&eve, &awareness(77, 2, forged)).is_none());
        // Garbage is dropped.
        assert!(d.stamp_awareness(&eve, &[0xff, 0xff]).is_none());
        // Leaving removes Alex's caret with a newer clock.
        let gone = decode(&d.remove_participant(alex.id).unwrap());
        assert_eq!(gone[&77], (2, "null".to_string()));
        assert!(d.remove_participant(alex.id).is_none());
    }
}
