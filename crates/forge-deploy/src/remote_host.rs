//! The client side of the Remote Host tool window: open server connections kept between
//! tree expansions ([`Connections`]), the operations of its context menu and Edit Remote
//! File, which edits a temporary copy in the user's cache folder and uploads it back on
//! save, unless the server copy changed since it was downloaded.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::browse::{self, RemoteNode};
use crate::config::DeployServer;
use crate::connect::Opened;
use crate::fs::ServerFs;
use crate::mapping::normalize;
use crate::{DeployError, Result};

struct Open {
    /// The server settings the connection was opened with (edits reconnect).
    server: DeployServer,
    opened: Opened,
}

/// Open connections by server id.
#[derive(Default)]
pub struct Connections {
    open: HashMap<String, Open>,
}

impl Connections {
    /// Run `op` on a connection to `server`: the open one when it still works and the server
    /// was not edited, else a new one from `connect`. A connection whose operation failed is
    /// closed, so the next call reconnects.
    pub fn with<T>(
        &mut self,
        server: &DeployServer,
        connect: impl FnOnce(&DeployServer) -> Result<Opened>,
        op: impl FnOnce(&mut dyn ServerFs, &str) -> Result<T>,
    ) -> Result<T> {
        let reuse = match self.open.get_mut(&server.id) {
            Some(o) if o.server == *server => o.opened.fs.stat("").is_ok(),
            _ => false,
        };
        if !reuse {
            self.open.remove(&server.id);
            let opened = connect(server)?;
            self.open.insert(
                server.id.clone(),
                Open {
                    server: server.clone(),
                    opened,
                },
            );
        }
        let open = self.open.get_mut(&server.id).expect("just opened");
        let root = open.opened.root.clone();
        let result = op(open.opened.fs.as_mut(), &root);
        if result.is_err() {
            self.open.remove(&server.id);
        }
        result
    }

    /// Close one server's connection, or all of them.
    pub fn close(&mut self, id: Option<&str>) {
        match id {
            Some(id) => {
                self.open.remove(id);
            }
            None => self.open.clear(),
        }
    }

    pub fn is_open(&self, id: &str) -> bool {
        self.open.contains_key(id)
    }
}

/// A folder of the tree (Refresh forgets cached listings first).
pub fn list(fs: &mut dyn ServerFs, server: &DeployServer, dir: &str) -> Result<Vec<RemoteNode>> {
    fs.invalidate();
    let dir = normalize(dir);
    let entries = fs.list(&dir)?;
    Ok(browse::nodes(server, &entries))
}

fn invalid(message: String) -> DeployError {
    DeployError::new(message)
}

/// New File / New Directory in `dir`; fails when the name is taken.
pub fn create(fs: &mut dyn ServerFs, dir: &str, name: &str, folder: bool) -> Result<String> {
    let path = browse::child_path(dir, name).map_err(invalid)?;
    if fs.stat(&path)?.is_some() {
        return Err(DeployError::new(format!(
            "{} already exists",
            fs.describe(&path)
        )));
    }
    if folder {
        fs.mkdir(&path)?;
    } else {
        fs.write(&path, b"")?;
    }
    Ok(path)
}

/// Rename a file or folder in its folder; fails when the new name is taken.
pub fn rename(fs: &mut dyn ServerFs, path: &str, new_name: &str) -> Result<String> {
    let target = browse::renamed_path(path, new_name).map_err(invalid)?;
    let path = normalize(path);
    if target == path {
        return Ok(target);
    }
    if fs.stat(&path)?.is_none() {
        return Err(DeployError::new(format!(
            "{} does not exist",
            fs.describe(&path)
        )));
    }
    // A different case of the same name is a rename on case-insensitive servers.
    if !target.eq_ignore_ascii_case(&path) && fs.stat(&target)?.is_some() {
        return Err(DeployError::new(format!(
            "{} already exists",
            fs.describe(&target)
        )));
    }
    fs.rename(&path, &target)?;
    Ok(target)
}

/// Delete files and folders (folders with everything in them). The root is never deleted.
pub fn delete(fs: &mut dyn ServerFs, paths: &[String]) -> Result<usize> {
    let mut deleted = 0;
    for path in paths {
        let path = normalize(path);
        if path.is_empty() {
            return Err(DeployError::new("The root folder is never deleted"));
        }
        if fs.stat(&path)?.is_some() {
            fs.remove_all(&path)?;
            deleted += 1;
        }
    }
    Ok(deleted)
}

/// A remote file for Compare with Local Version and the editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteText {
    /// `None` for binary files.
    pub text: Option<String>,
    pub binary: bool,
    pub size: u64,
}

pub fn read_text(fs: &mut dyn ServerFs, path: &str) -> Result<RemoteText> {
    let bytes = fs.read(&normalize(path))?;
    let size = bytes.len() as u64;
    Ok(match String::from_utf8(bytes) {
        Ok(text) => RemoteText {
            text: Some(text),
            binary: false,
            size,
        },
        Err(_) => RemoteText {
            text: None,
            binary: true,
            size,
        },
    })
}

// ---- Edit Remote File ---------------------------------------------------------------------

/// A remote file being edited through a temporary copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditedFile {
    pub server_id: String,
    /// Relative to the server's root path.
    pub path: String,
    /// The temporary copy (below the user's cache folder, never in the project).
    pub temp: PathBuf,
    /// The server copy the temporary copy is based on.
    pub digest: u64,
    pub size: u64,
}

/// Where the temporary copy of `path` on `server_id` lives below `cache` (the folder
/// structure is kept, so the editor shows the right file type and name).
pub fn temp_path(cache: &Path, server_id: &str, path: &str) -> PathBuf {
    let safe: String = server_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut out = cache.join("remote-edit").join(if safe.is_empty() {
        "server".to_string()
    } else {
        safe
    });
    for part in normalize(path).split('/').filter(|p| *p != "..") {
        out.push(part);
    }
    out
}

/// Download `path` into its temporary copy below `cache`.
pub fn start_edit(
    fs: &mut dyn ServerFs,
    server_id: &str,
    path: &str,
    cache: &Path,
) -> Result<EditedFile> {
    let path = normalize(path);
    match fs.stat(&path)? {
        Some(e) if e.dir => {
            return Err(DeployError::new(format!(
                "{} is a folder",
                fs.describe(&path)
            )))
        }
        None => {
            return Err(DeployError::new(format!(
                "{} does not exist",
                fs.describe(&path)
            )))
        }
        Some(_) => {}
    }
    let bytes = fs.read(&path)?;
    let temp = temp_path(cache, server_id, &path);
    if let Some(dir) = temp.parent() {
        std::fs::create_dir_all(dir).map_err(|e| DeployError::context(dir.display(), e))?;
    }
    std::fs::write(&temp, &bytes).map_err(|e| DeployError::context(temp.display(), e))?;
    Ok(EditedFile {
        server_id: server_id.to_string(),
        path,
        temp,
        digest: forge_diff::digest(&bytes),
        size: bytes.len() as u64,
    })
}

/// What saving an edited remote file did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum SaveOutcome {
    Uploaded,
    /// The server copy changed (or was deleted) since it was downloaded; nothing was
    /// uploaded. Upload again with `force` to overwrite it.
    #[serde(rename_all = "camelCase")]
    Conflict {
        deleted: bool,
    },
}

/// Upload the temporary copy's `data` back, after the conflict check (skipped with `force`).
pub fn save_edit(
    fs: &mut dyn ServerFs,
    edited: &mut EditedFile,
    data: &[u8],
    force: bool,
) -> Result<SaveOutcome> {
    if !force {
        match fs.stat(&edited.path)? {
            None => return Ok(SaveOutcome::Conflict { deleted: true }),
            Some(e) if e.dir => return Ok(SaveOutcome::Conflict { deleted: true }),
            Some(_) => {
                let current = fs.read(&edited.path)?;
                if forge_diff::digest(&current) != edited.digest {
                    return Ok(SaveOutcome::Conflict { deleted: false });
                }
            }
        }
    }
    fs.mkdir_all(crate::mapping::parent(&edited.path))?;
    fs.write(&edited.path, data)?;
    edited.digest = forge_diff::digest(data);
    edited.size = data.len() as u64;
    Ok(SaveOutcome::Uploaded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::LocalFolder;
    use std::cell::Cell;

    fn local_server(id: &str, root: &Path) -> DeployServer {
        DeployServer {
            id: id.into(),
            name: id.into(),
            kind: crate::ServerKind::Local,
            root_path: root.display().to_string(),
            ..DeployServer::default()
        }
    }

    fn opener<'a>(
        count: &'a Cell<usize>,
        root: &'a Path,
    ) -> impl Fn(&DeployServer) -> Result<Opened> + 'a {
        move |_| {
            count.set(count.get() + 1);
            Ok(Opened {
                fs: Box::new(LocalFolder::new(root)),
                root: root.display().to_string(),
                welcome: None,
                inside_project: None,
            })
        }
    }

    #[test]
    fn connections_are_reused_until_the_server_changes_or_fails() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        let count = Cell::new(0);
        let mut pool = Connections::default();
        let server = local_server("s", dir.path());
        let names = |fs: &mut dyn ServerFs, root: &str| {
            assert!(!root.is_empty());
            Ok(fs.list("")?.len())
        };
        assert_eq!(
            pool.with(&server, opener(&count, dir.path()), names)
                .unwrap(),
            1
        );
        assert_eq!(
            pool.with(&server, opener(&count, dir.path()), names)
                .unwrap(),
            1
        );
        assert_eq!(count.get(), 1);
        assert!(pool.is_open("s"));
        // Editing the server reconnects.
        let mut edited = server.clone();
        edited.web_url = "http://localhost".into();
        pool.with(&edited, opener(&count, dir.path()), names)
            .unwrap();
        assert_eq!(count.get(), 2);
        // A failed operation closes the connection.
        let failed: Result<()> = pool.with(&edited, opener(&count, dir.path()), |fs, _| {
            fs.read("missing").map(|_| ())
        });
        assert!(failed.is_err());
        assert!(!pool.is_open("s"));
        pool.with(&edited, opener(&count, dir.path()), names)
            .unwrap();
        assert_eq!(count.get(), 3);
        pool.close(None);
        assert!(!pool.is_open("s"));
    }

    #[test]
    fn context_menu_operations() {
        let dir = tempfile::tempdir().unwrap();
        let server = DeployServer {
            mappings: vec![crate::Mapping {
                local: "public".into(),
                deployment: "htdocs".into(),
                web: "".into(),
            }],
            ..local_server("s", dir.path())
        };
        let mut fs = LocalFolder::new(dir.path());
        assert_eq!(create(&mut fs, "", "htdocs", true).unwrap(), "htdocs");
        assert_eq!(
            create(&mut fs, "htdocs", "index.php", false).unwrap(),
            "htdocs/index.php"
        );
        assert!(create(&mut fs, "htdocs", "index.php", false)
            .unwrap_err()
            .to_string()
            .contains("already exists"));
        assert!(create(&mut fs, "htdocs", "../x", false).is_err());
        let listed = list(&mut fs, &server, "htdocs").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].local.as_deref(), Some("public/index.php"));
        assert!(list(&mut fs, &server, "").unwrap()[0].mapping_root);

        create(&mut fs, "htdocs", "old.php", false).unwrap();
        assert!(rename(&mut fs, "htdocs/old.php", "index.php").is_err());
        assert_eq!(
            rename(&mut fs, "htdocs/old.php", "new.php").unwrap(),
            "htdocs/new.php"
        );
        assert_eq!(
            rename(&mut fs, "htdocs/new.php", "new.php").unwrap(),
            "htdocs/new.php"
        );
        assert!(rename(&mut fs, "htdocs/gone.php", "x.php").is_err());
        assert!(rename(&mut fs, "", "x").is_err());
        assert_eq!(
            read_text(&mut fs, "htdocs/new.php")
                .unwrap()
                .text
                .as_deref(),
            Some("")
        );
        fs.write("htdocs/logo.png", &[0x89, 0xff, 0x00]).unwrap();
        let png = read_text(&mut fs, "htdocs/logo.png").unwrap();
        assert!(png.binary && png.text.is_none() && png.size == 3);

        assert!(delete(&mut fs, &["".into()]).is_err());
        assert_eq!(
            delete(&mut fs, &["htdocs".into(), "missing".into()]).unwrap(),
            1
        );
        assert!(fs.list("").unwrap().is_empty());
    }

    #[test]
    fn edited_files_upload_back_unless_the_server_copy_changed() {
        let server_dir = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let mut fs = LocalFolder::new(server_dir.path());
        fs.mkdir_all("config").unwrap();
        fs.write("config/app.php", b"<?php return [];").unwrap();

        let mut edited = start_edit(&mut fs, "shop/prod", "config/app.php", cache.path()).unwrap();
        assert_eq!(
            edited.temp,
            cache.path().join("remote-edit/shop_prod/config/app.php")
        );
        assert_eq!(
            std::fs::read_to_string(&edited.temp).unwrap(),
            "<?php return [];"
        );
        assert!(start_edit(&mut fs, "s", "config", cache.path()).is_err());
        assert!(start_edit(&mut fs, "s", "nope.php", cache.path()).is_err());

        assert_eq!(
            save_edit(
                &mut fs,
                &mut edited,
                b"<?php return ['debug' => true];",
                false
            )
            .unwrap(),
            SaveOutcome::Uploaded
        );
        // Saving again is fine: the base moved with the upload.
        save_edit(
            &mut fs,
            &mut edited,
            b"<?php return ['debug' => false];",
            false,
        )
        .unwrap();

        // Someone else changed the file on the server.
        fs.write("config/app.php", b"<?php return ['hotfix' => 1];")
            .unwrap();
        assert_eq!(
            save_edit(&mut fs, &mut edited, b"mine", false).unwrap(),
            SaveOutcome::Conflict { deleted: false }
        );
        assert_eq!(
            fs.read("config/app.php").unwrap(),
            b"<?php return ['hotfix' => 1];"
        );
        assert_eq!(
            save_edit(&mut fs, &mut edited, b"mine", true).unwrap(),
            SaveOutcome::Uploaded
        );
        assert_eq!(fs.read("config/app.php").unwrap(), b"mine");

        fs.remove_all("config").unwrap();
        assert_eq!(
            save_edit(&mut fs, &mut edited, b"again", false).unwrap(),
            SaveOutcome::Conflict { deleted: true }
        );
        assert_eq!(
            save_edit(&mut fs, &mut edited, b"again", true).unwrap(),
            SaveOutcome::Uploaded
        );
        assert_eq!(fs.read("config/app.php").unwrap(), b"again");
        assert_eq!(
            serde_json::to_value(SaveOutcome::Conflict { deleted: true }).unwrap(),
            serde_json::json!({ "status": "conflict", "deleted": true })
        );
        assert!(temp_path(cache.path(), "", "../../etc/passwd")
            .starts_with(cache.path().join("remote-edit/server")));
    }
}
