//! Where a launch goes: Forge started (or handed over to the running instance) with files,
//! folders or links, from a terminal, the file manager ("Open With", "Open Folder in Forge"),
//! a `forge://` link or the macOS open-file event.
//!
//! - A **folder** opens as a project (asking "This Window / New Window" when a project is
//!   open; that question lives in the UI).
//! - A **file** of the open project (or of a folder in the same launch) opens in the project
//!   window. Every other file opens in the lightweight editor window (LightEdit), like
//!   PHPStorm; `-e` / `--edit` sends every file there.
//! - A `forge://open?file=<folder>` link opens the folder as a project after the user
//!   confirms (links may come from any web page).

use std::path::{Path, PathBuf};

use crate::cli::LaunchTarget;
use crate::request::{strip_file_url, LinkAction, OpenRequest};
use crate::resolve::normalize_lexically;

/// Everything one launch asks for, sorted by where it opens.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchPlan {
    /// Folders from the command line or the file manager, opened as projects.
    pub projects: Vec<PathBuf>,
    /// Folders named by `forge://` links: opened as projects after the user confirms.
    pub link_projects: Vec<PathBuf>,
    /// Files of a project root ([`plan_launch`]'s `roots`): opened in the project window.
    pub project_files: Vec<OpenRequest>,
    /// Files opened in the LightEdit window.
    pub light_edit: Vec<OpenRequest>,
    /// Other `forge://` links (files, symbols).
    pub links: Vec<LinkAction>,
}

impl LaunchPlan {
    /// The launch only opens LightEdit files: at startup Forge then shows the LightEdit
    /// window alone (no project window, no last project).
    pub fn light_edit_only(&self) -> bool {
        !self.light_edit.is_empty()
            && self.projects.is_empty()
            && self.link_projects.is_empty()
            && self.project_files.is_empty()
            && self.links.is_empty()
    }

    pub fn is_empty(&self) -> bool {
        self.projects.is_empty()
            && self.link_projects.is_empty()
            && self.project_files.is_empty()
            && self.light_edit.is_empty()
            && self.links.is_empty()
    }
}

/// `-e` / `--edit` among the arguments: open every file in LightEdit (PHPStorm's flag).
pub fn wants_light_edit<S: AsRef<str>>(args: &[S]) -> bool {
    args.iter()
        .any(|a| matches!(a.as_ref(), "-e" | "--edit" | "--light-edit"))
}

/// The folder a `forge://open?file=…` link names, when it names an existing folder by an
/// absolute path (relative paths of links resolve against projects, never against the
/// current directory).
pub fn link_folder(link: &LinkAction) -> Option<PathBuf> {
    let LinkAction::Open(request) = link else {
        return None;
    };
    let path = PathBuf::from(strip_file_url(request.file.trim()));
    if !path.is_absolute() || !path.is_dir() {
        return None;
    }
    Some(forge_core::paths::canonicalize(&path).unwrap_or(path))
}

fn within(file: &Path, root: &Path) -> bool {
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| normalize_lexically(p));
    canon(file).starts_with(canon(root))
}

/// Sort the parsed arguments of one launch. `roots` are the folders whose files open in the
/// project window: the open project, and at startup also the folders of this launch (the
/// first one opens right away, so `forge . src/main.rs` opens the file in it).
pub fn plan_launch(
    targets: Vec<LaunchTarget>,
    roots: &[PathBuf],
    force_light_edit: bool,
    folders_are_roots: bool,
) -> LaunchPlan {
    let mut plan = LaunchPlan::default();
    let mut files = Vec::new();
    for target in targets {
        match target {
            LaunchTarget::Project(dir) => plan.projects.push(dir),
            LaunchTarget::File(request) => files.push(request),
            LaunchTarget::Link(link) => match link_folder(&link) {
                Some(dir) => plan.link_projects.push(dir),
                None => plan.links.push(link),
            },
        }
    }
    let mut all_roots: Vec<PathBuf> = roots.to_vec();
    if folders_are_roots {
        all_roots.extend(plan.projects.first().cloned());
    }
    for request in files {
        let in_project = !force_light_edit
            && all_roots
                .iter()
                .any(|root| within(Path::new(&request.file), root));
        if in_project {
            plan.project_files.push(request);
        } else {
            plan.light_edit.push(request);
        }
    }
    plan
}

/// The paths of a macOS open-file event (`RunEvent::Opened`, Finder "Open With", files and
/// folders dropped on the Dock icon), as launcher arguments for
/// [`crate::cli::parse_launch_args`]. `forge://` URLs are left out: the deep-link plugin
/// receives them from the same event.
pub fn opened_urls_to_args<I, S>(urls: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    urls.into_iter()
        .filter_map(|url| {
            let url = url.as_ref().trim();
            let lower = url.get(..7).map(str::to_ascii_lowercase);
            (lower.as_deref() == Some("file://")).then(|| {
                let path = strip_file_url(&format!("file://{}", &url[7..]));
                // Finder sends folders with a trailing slash.
                match path.trim_end_matches('/') {
                    "" => "/".to_string(),
                    trimmed => trimmed.to_string(),
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::parse_launch_args;
    use std::fs;

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let base = forge_core::paths::canonicalize(dir.path()).unwrap();
        fs::create_dir_all(base.join("shop/src")).unwrap();
        fs::create_dir_all(base.join("notes")).unwrap();
        fs::write(base.join("shop/src/Cart.php"), "<?php").unwrap();
        fs::write(base.join("notes/todo.md"), "- milk").unwrap();
        fs::write(base.join("hosts"), "127.0.0.1 localhost").unwrap();
        (dir, base)
    }

    fn plan(args: &[&str], base: &Path, roots: &[PathBuf], startup: bool) -> LaunchPlan {
        plan_launch(
            parse_launch_args(args, base),
            roots,
            wants_light_edit(args),
            startup,
        )
    }

    #[test]
    fn folders_open_as_projects_and_other_files_in_light_edit() {
        let (_d, base) = fixture();
        let p = plan(&["shop", "notes/todo.md", "hosts"], &base, &[], false);
        assert_eq!(p.projects, vec![base.join("shop")]);
        assert!(p.project_files.is_empty());
        let light: Vec<_> = p.light_edit.iter().map(|r| r.file.clone()).collect();
        assert_eq!(
            light,
            vec![
                base.join("notes/todo.md").display().to_string(),
                base.join("hosts").display().to_string()
            ]
        );
        assert!(!p.light_edit_only());
    }

    #[test]
    fn files_of_the_open_project_stay_in_the_project_window() {
        let (_d, base) = fixture();
        let roots = [base.join("shop")];
        let p = plan(
            &["shop/src/Cart.php:12", "notes/todo.md"],
            &base,
            &roots,
            false,
        );
        assert_eq!(p.project_files.len(), 1);
        assert_eq!(p.project_files[0].line, Some(12));
        assert_eq!(p.light_edit.len(), 1);
    }

    #[test]
    fn at_startup_files_of_the_launched_folder_open_in_it() {
        let (_d, base) = fixture();
        let p = plan(&["shop", "shop/src/Cart.php"], &base, &[], true);
        assert_eq!(p.projects, vec![base.join("shop")]);
        assert_eq!(p.project_files.len(), 1);
        assert!(p.light_edit.is_empty());
        // Later launches do not count their folders: the folder is not open yet.
        let p = plan(&["shop", "shop/src/Cart.php"], &base, &[], false);
        assert_eq!(p.light_edit.len(), 1);
    }

    #[test]
    fn edit_flag_sends_every_file_to_light_edit() {
        let (_d, base) = fixture();
        let roots = [base.join("shop")];
        let p = plan(&["-e", "shop/src/Cart.php"], &base, &roots, false);
        assert!(p.project_files.is_empty());
        assert_eq!(p.light_edit.len(), 1);
        assert!(p.light_edit_only());
    }

    #[test]
    fn open_with_on_files_only_is_light_edit_only() {
        let (_d, base) = fixture();
        let todo = base.join("notes/todo.md");
        let p = plan(&[todo.to_str().unwrap()], Path::new("/"), &[], true);
        assert!(p.light_edit_only());
        assert!(!plan(&["missing.txt"], &base, &[], true).light_edit_only());
        assert!(plan(&["missing.txt"], &base, &[], true).is_empty());
    }

    #[test]
    fn file_urls_from_file_managers_are_paths() {
        let (_d, base) = fixture();
        let url = format!("file://{}", base.join("notes/todo.md").display());
        let p = plan(&[url.as_str()], Path::new("/"), &[], true);
        assert_eq!(
            p.light_edit[0].file,
            base.join("notes/todo.md").display().to_string()
        );
        let url = format!("file://{}", base.join("shop").display());
        let p = plan(&[url.as_str()], Path::new("/"), &[], true);
        assert_eq!(p.projects, vec![base.join("shop")]);
    }

    #[test]
    fn links_to_folders_open_projects_after_confirmation() {
        let (_d, base) = fixture();
        let folder = format!("forge://open?file={}", base.join("shop").display());
        let file = format!(
            "forge://open?file={}",
            base.join("shop/src/Cart.php").display()
        );
        let p = plan(&[folder.as_str(), file.as_str()], &base, &[], false);
        assert_eq!(p.link_projects, vec![base.join("shop")]);
        assert!(p.projects.is_empty());
        assert_eq!(p.links.len(), 1);
        // The folder keeps its ordinary spelling (no `\\?\` prefix on Windows, #215).
        let shop = LinkAction::Open(OpenRequest::new(base.join("shop").display().to_string()));
        let found = link_folder(&shop).unwrap();
        assert!(!found.to_string_lossy().starts_with(r"\\?\"), "{found:?}");
        // Relative folders in links are not resolved against the current directory.
        let relative = LinkAction::Open(OpenRequest::new("shop"));
        assert_eq!(link_folder(&relative), None);
    }

    #[test]
    fn macos_open_events_map_to_paths() {
        let args = opened_urls_to_args([
            "file:///Users/me/notes/todo%20list.md",
            "file:///Users/me/Projects/shop/",
            "FILE:///Users/me/a.txt",
            "forge://open?file=/x.php",
            "https://example.com/",
            "file:///",
        ]);
        assert_eq!(
            args,
            vec![
                "/Users/me/notes/todo list.md",
                "/Users/me/Projects/shop",
                "/Users/me/a.txt",
                "/",
            ]
        );
    }

    #[test]
    fn macos_open_event_paths_route_like_arguments() {
        let (_d, base) = fixture();
        let urls = [
            format!("file://{}/", base.join("shop").display()),
            format!("file://{}", base.join("hosts").display()),
        ];
        let args = opened_urls_to_args(&urls);
        let p = plan_launch(parse_launch_args(&args, Path::new("/")), &[], false, false);
        assert_eq!(p.projects, vec![base.join("shop")]);
        assert_eq!(p.light_edit.len(), 1);
    }
}
