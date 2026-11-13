//! Local History, like PHPStorm's: Forge records the contents of project files on every
//! save, before and after refactorings and VCS operations, on external changes the file
//! watcher finds, and when files or folders are created, deleted, renamed or moved. The
//! history is kept per project outside the project (`<data dir>/forge/projects/<key>/
//! local-history`), contents compressed and deduplicated by hash, and cleaned up after a
//! number of working days or when it grows past a size limit.
//!
//! - [`model`]: change sets of path entries (state before / after), labels, limits.
//! - [`storage`]: the in-memory store and [`DiskStorage`] (append-only change log plus
//!   zlib-compressed contents).
//! - [`LocalHistory`]: recording, Show History of a file or folder, contents at a revision,
//!   revert plans, Show History for Selection, Recent Changes, Create Patch and cleanup.
//!
//! A revision is the state *before* its change set (for a label: at the label), so the
//! newest revision of a file compared with the file now shows its latest change.

pub mod history;
pub mod model;
pub mod storage;
pub mod text;

pub use history::{
    BlockRevision, ChangeFile, FolderChange, LocalHistory, PurgeStats, RecentChange, RevertAction,
    RevertOp, RevertPlan, Revision, RevisionContent, SelectionHistory,
};
pub use model::{
    ChangeKind, ChangeSet, Content, Entry, FileState, Limits, NewChange, NewEntry, StateKind,
};
#[cfg(feature = "disk")]
pub use storage::DiskStorage;
pub use storage::{content_hash, MemStorage, Storage};

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400_000;

    fn bytes(t: &str) -> Content {
        Content::Bytes(t.as_bytes().to_vec())
    }

    fn change(kind: ChangeKind, name: &str, time: i64, entries: Vec<NewEntry>) -> NewChange {
        NewChange {
            kind,
            name: name.into(),
            time,
            group: None,
            entries,
        }
    }

    fn save(
        h: &mut LocalHistory<impl Storage>,
        path: &str,
        before: &str,
        after: &str,
        time: i64,
    ) -> u64 {
        h.record(change(
            ChangeKind::Save,
            "Saved",
            time,
            vec![NewEntry::new(path, bytes(before), bytes(after))],
        ))
        .unwrap()
        .unwrap()
    }

    fn text(h: &LocalHistory<impl Storage>, path: &str, rev: u64) -> Option<String> {
        h.content(path, rev).unwrap().text
    }

    fn mem() -> LocalHistory<MemStorage> {
        LocalHistory::open(MemStorage::default(), Limits::default()).unwrap()
    }

    #[test]
    fn saves_are_revisions_of_the_state_before_them() {
        let mut h = mem();
        let a = save(&mut h, "a.php", "one", "two", 10);
        let b = save(&mut h, "a.php", "two", "three", 20);
        let revs = h.history("a.php", false, "").unwrap();
        assert_eq!(revs.iter().map(|r| r.id).collect::<Vec<_>>(), [b, a]);
        assert_eq!(text(&h, "a.php", b).as_deref(), Some("two"));
        assert_eq!(text(&h, "a.php", a).as_deref(), Some("one"));
        assert_eq!(h.content("a.php", b + 1).unwrap().state, StateKind::Current);
        // Saving the same content again records nothing.
        assert!(h
            .record(change(
                ChangeKind::Save,
                "Saved",
                30,
                vec![NewEntry::new("a.php", bytes("three"), bytes("three"))]
            ))
            .unwrap()
            .is_none());
        // An unknown "before" is the last recorded state.
        let c = h
            .record(change(
                ChangeKind::Save,
                "Saved",
                40,
                vec![NewEntry::new("a.php", Content::Unknown, bytes("four"))],
            ))
            .unwrap()
            .unwrap();
        assert_eq!(text(&h, "a.php", c).as_deref(), Some("three"));
        // The text filter matches names and contents.
        assert_eq!(h.history("a.php", false, "TWO").unwrap().len(), 1);
        assert_eq!(h.history("a.php", false, "saved").unwrap().len(), 3);
    }

    #[test]
    fn excluded_large_and_binary_files_are_not_recorded() {
        let mut h = LocalHistory::open(
            MemStorage::default(),
            Limits {
                max_file_size: 10,
                ..Limits::default()
            },
        )
        .unwrap();
        let none = |h: &mut LocalHistory<MemStorage>, p: &str, after: &str| {
            h.record(change(
                ChangeKind::Save,
                "Saved",
                1,
                vec![NewEntry::new(p, Content::Unknown, bytes(after))],
            ))
            .unwrap()
        };
        assert!(none(&mut h, "vendor/x.php", "a").is_none());
        assert!(none(&mut h, "big.txt", "0123456789abc").is_none());
        assert!(none(&mut h, "img.png", "\0PNG").is_none());
        assert!(none(&mut h, "ok.txt", "small").is_some());
    }

    #[test]
    fn groups_merge_an_operation_into_one_change_set() {
        let mut h = mem();
        let mut c = change(
            ChangeKind::Refactoring,
            "Refactoring: Rename User to Account",
            5,
            vec![NewEntry::new("a.php", bytes("User"), bytes("Account"))],
        );
        c.group = Some(7);
        let id = h.record(c.clone()).unwrap().unwrap();
        c.entries = vec![
            NewEntry::new("b.php", bytes("new User"), bytes("new Account")),
            NewEntry::new("a.php", Content::Unknown, bytes("Account!")),
        ];
        assert_eq!(h.record(c.clone()).unwrap(), Some(id));
        let set = &h.sets()[0];
        assert_eq!(set.entries.len(), 2);
        assert_eq!(text(&h, "a.php", id).as_deref(), Some("User"));
        h.close_group(7);
        c.entries = vec![NewEntry::new("b.php", Content::Unknown, bytes("x"))];
        assert_ne!(h.record(c).unwrap(), Some(id));
    }

    #[test]
    fn renames_are_followed_and_folders_revert_with_deleted_files() {
        let mut h = mem();
        let created = h
            .record(change(
                ChangeKind::Create,
                "Create file Old.php",
                1,
                vec![NewEntry::new(
                    "src/Old.php",
                    Content::Absent,
                    bytes("class Old {}"),
                )],
            ))
            .unwrap()
            .unwrap();
        let label = h.put_label("Before rename", 2).unwrap();
        let mut moved = NewEntry::new("lib/New.php", Content::Absent, bytes("class Old {}"));
        moved.from = Some("src/Old.php".into());
        let renamed = h
            .record(change(
                ChangeKind::Move,
                "Move Old.php to lib",
                3,
                vec![
                    NewEntry::new("src/Old.php", bytes("class Old {}"), Content::Absent),
                    moved,
                ],
            ))
            .unwrap()
            .unwrap();
        let saved = save(&mut h, "lib/New.php", "class Old {}", "class New {}", 4);
        let deleted = h
            .record(change(
                ChangeKind::Delete,
                "Delete gone.txt",
                5,
                vec![NewEntry::new("src/gone.txt", bytes("bye"), Content::Absent)],
            ))
            .unwrap()
            .unwrap();

        let revs = h.history("lib/New.php", false, "").unwrap();
        let ids: Vec<u64> = revs.iter().map(|r| r.id).collect();
        assert_eq!(ids, [saved, renamed, label, created]);
        assert_eq!(revs[3].path, "src/Old.php");
        assert_eq!(
            text(&h, &revs[2].path, label).as_deref(),
            Some("class Old {}")
        );
        assert_eq!(
            h.content(&revs[3].path, created).unwrap().state,
            StateKind::Absent
        );

        // Reverting the file keeps its name and puts the old content back.
        let plan = h.revert_file("lib/New.php", "lib/New.php", saved).unwrap();
        assert_eq!(
            plan.ops,
            [RevertOp {
                path: "lib/New.php".into(),
                action: RevertAction::Write(b"class Old {}".to_vec())
            }]
        );

        // The project at the label: Old.php back, New.php and the deletion undone.
        let changes = h.folder_changes("", label);
        assert_eq!(changes.len(), 3);
        let plan = h.revert_folder("", label).unwrap();
        let ops: Vec<(String, &RevertAction)> = plan
            .ops
            .iter()
            .map(|o| (o.path.clone(), &o.action))
            .collect();
        assert_eq!(
            ops,
            [
                (
                    "src/Old.php".into(),
                    &RevertAction::Write(b"class Old {}".to_vec())
                ),
                ("src/gone.txt".into(), &RevertAction::Write(b"bye".to_vec())),
                ("lib/New.php".into(), &RevertAction::Delete),
            ]
        );
        assert!(plan.skipped.is_empty());

        // Folder history lists the changes below the folder.
        let src = h.history("src", true, "").unwrap();
        assert_eq!(
            src.iter().map(|r| r.id).collect::<Vec<_>>(),
            [deleted, renamed, label, created]
        );
        assert_eq!(src[1].files, ["src/Old.php"]);

        // Recent Changes and the revert of one entry.
        let recent = h.recent_changes(10);
        assert_eq!(recent[0].name, "Delete gone.txt");
        let files = h.change_files(deleted, deleted);
        assert_eq!(files[0].before, StateKind::File);
        assert_eq!(files[0].after, StateKind::Current);
        let plan = h.revert_change(deleted, deleted).unwrap();
        assert_eq!(plan.ops[0].action, RevertAction::Write(b"bye".to_vec()));

        // Create Patch between the label and now.
        let patch = h
            .patch("lib/New.php", false, "src/Old.php", label, |_| {
                Some("class New {}".into())
            })
            .unwrap();
        assert!(patch.contains("-class Old {}"), "{patch}");
        assert!(patch.contains("+class New {}"), "{patch}");
    }

    #[test]
    fn selection_history_lists_revisions_where_the_lines_changed() {
        let mut h = mem();
        let v1 = "a\nb\nc\n";
        let v2 = "a\nB\nc\n";
        let v3 = "x\na\nB\nc\n";
        let r1 = save(&mut h, "f.txt", v1, v2, 1);
        let r2 = save(&mut h, "f.txt", v2, v3, 2);
        // Line 3 ("B") now.
        let sel = h.selection_history("f.txt", v3, 3, 3).unwrap();
        assert_eq!(sel.current, "B\n");
        // r2 only inserted a line above: the block did not change, so it is not listed.
        assert_eq!(sel.revisions.len(), 1);
        assert_eq!(sel.revisions[0].id, r1);
        assert_eq!(sel.revisions[0].text, "b\n");
        assert_eq!(sel.revisions[0].start_line, 2);
        let _ = r2;
    }

    #[test]
    fn baselines_supply_the_before_and_detect_external_changes() {
        let mut h = mem();
        assert!(h.baseline("a.txt", b"one", 1).unwrap().is_none());
        assert!(h.baseline("b.txt", b"bee", 1).unwrap().is_none());
        assert_eq!(h.sets().len(), 1, "baselines in a row share a set");
        assert!(h.history("a.txt", false, "").unwrap().is_empty());
        let id = h
            .record(change(
                ChangeKind::Save,
                "Saved",
                2,
                vec![NewEntry::new("a.txt", Content::Unknown, bytes("two"))],
            ))
            .unwrap()
            .unwrap();
        assert_eq!(text(&h, "a.txt", id).as_deref(), Some("one"));
        // Reading a different content than recorded: an external change.
        let ext = h.baseline("a.txt", b"three", 3).unwrap().unwrap();
        let revs = h.history("a.txt", false, "").unwrap();
        assert_eq!(revs[0].id, ext);
        assert_eq!(revs[0].name, "External change");
        assert_eq!(text(&h, "a.txt", ext).as_deref(), Some("two"));
        assert!(h.baseline("a.txt", b"three", 4).unwrap().is_none());
    }

    #[test]
    fn saves_in_a_row_are_one_recent_change() {
        let mut h = mem();
        save(&mut h, "a", "1", "2", 1000);
        save(&mut h, "b", "1", "2", 2000);
        h.record(change(
            ChangeKind::External,
            "External change",
            3000,
            vec![NewEntry::new("c", bytes("1"), bytes("2"))],
        ))
        .unwrap();
        save(&mut h, "a", "2", "3", 4000);
        let recent = h.recent_changes(10);
        assert_eq!(recent.len(), 3);
        assert_eq!(
            (recent[2].from, recent[2].to, recent[2].file_count),
            (1, 2, 2)
        );
        assert_eq!(recent[2].files, ["a", "b"]);
        assert_eq!(h.change_files(1, 2).len(), 2);
    }

    #[test]
    fn purge_keeps_working_days_and_fits_the_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let open = || LocalHistory::open(DiskStorage::new(dir.path()), Limits::default()).unwrap();
        let mut h = open();
        // Activity on days 1, 2, 10 and 11 (gaps are not working days).
        for (i, day) in [1, 2, 10, 11].iter().enumerate() {
            save(
                &mut h,
                "a.txt",
                &format!("v{i}"),
                &format!("v{}", i + 1),
                day * DAY + 5,
            );
        }
        h.put_label("release", 11 * DAY + 6).unwrap();
        let stats = h.purge(3, u64::MAX).unwrap();
        assert_eq!(stats.sets, 1);
        assert_eq!(stats.contents, 1, "v0 is no longer referenced");
        let h2 = open();
        assert_eq!(h2.sets().len(), 4);
        assert_eq!(text(&h2, "a.txt", h2.sets()[0].id).as_deref(), Some("v1"));
        // A tiny size limit keeps only the newest change set.
        let mut h3 = open();
        h3.purge(30, 1).unwrap();
        assert_eq!(h3.sets().len(), 1);
        assert_eq!(h3.sets()[0].kind, ChangeKind::Label);
        assert!(h3.storage().blob_sizes().unwrap().is_empty());
        // Recording goes on with fresh ids.
        let id = save(&mut h3, "a.txt", "v4", "v5", 12 * DAY);
        assert!(id > h3.sets()[0].id);
    }
}
