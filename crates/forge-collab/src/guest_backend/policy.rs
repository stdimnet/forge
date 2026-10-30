//! The permission check for [`HostCall`]s: [`Policy::check_call`]. It looks at one call and
//! the guest's permissions, never at the file system; what a path resolves to (symlinks) is
//! checked where the file is touched, in [`super::host`].

use std::sync::OnceLock;

use forge_backend::frameworks::{DoctrineQuery, SymfonyQuery};
use forge_backend::protocol::PhpCall;
use forge_backend::FrameworkCall;

use super::calls::{GitAction, GitQuery, HostCall, LspQuery};
use crate::excludes::Excludes;
use crate::permissions::{is_plain_relative, Denied, Policy, RunAccess, VcsAccess};

/// Files that configure what runs on the host. Changing them lets a guest turn the host's
/// next Run, commit or install into the guest's command, so they need Full access
/// (Run Full), not just Files Edit. Same pattern syntax as the excluded files.
pub const PROTECTED_FILES: &[&str] = &[
    ".forge/runConfigurations*.toml",
    ".forge/settings.toml",
    ".vscode/",
    ".idea/",
    ".husky/",
    "composer.json",
    "package.json",
    "Makefile",
    "artisan",
    "bin/console",
];

fn protected() -> &'static Excludes {
    static PROTECTED: OnceLock<Excludes> = OnceLock::new();
    PROTECTED.get_or_init(|| Excludes::new(PROTECTED_FILES).expect("protected patterns compile"))
}

fn denied(msg: impl Into<String>) -> Denied {
    Denied(msg.into())
}

/// A revision, branch or remote name git cannot mistake for an option or a `rev:path`
/// expression.
pub fn is_safe_rev(rev: &str) -> bool {
    !rev.is_empty()
        && rev.len() <= 1024
        && !rev.starts_with('-')
        && !rev.contains(':')
        && !rev.chars().any(|c| c.is_control() || c.is_whitespace())
}

fn rev(r: &str) -> Result<(), Denied> {
    if is_safe_rev(r) {
        Ok(())
    } else {
        Err(denied(format!("\"{r}\" is not a valid revision")))
    }
}

fn opt_rev(r: &Option<String>) -> Result<(), Denied> {
    r.as_deref().filter(|s| !s.is_empty()).map_or(Ok(()), rev)
}

/// A pathspec as git gets it (relative to the repository): the workspace folder itself,
/// or a plain relative path. Pathspec magic (`:`) and `..` never pass.
fn pathspec(p: &str) -> Result<(), Denied> {
    if p.is_empty() || p == "." {
        return Ok(());
    }
    let plain = p.strip_suffix('/').unwrap_or(p);
    if is_plain_relative(plain) {
        Ok(())
    } else {
        Err(denied(format!("\"{p}\" is not a path in the project")))
    }
}

fn pathspecs(ps: &[String]) -> Result<(), Denied> {
    if ps.len() > 10_000 {
        return Err(denied("too many paths"));
    }
    ps.iter().try_for_each(|p| pathspec(p))
}

/// A setting that names a project folder or file (a framework analysis setting).
fn project_setting(s: &str) -> Result<(), Denied> {
    if s.is_empty() {
        Ok(())
    } else {
        pathspec(s)
    }
}

fn identifier(s: &str) -> Result<(), Denied> {
    if !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        Ok(())
    } else {
        Err(denied(format!("\"{s}\" is not a valid name")))
    }
}

impl Policy {
    /// Whether `path` configures what runs on the host (see [`PROTECTED_FILES`]).
    pub fn is_protected(path: &str) -> bool {
        protected().is_excluded(path)
    }

    /// May the guest change `path` (create, write, rename, delete)? Visible, inside the
    /// project, Files Edit, and Run Full access for [`PROTECTED_FILES`].
    pub fn check_write_path(&self, path: &str) -> Result<(), Denied> {
        self.check_path(path)?;
        if !self.permissions.can_edit_files() {
            return Err(denied("Your permission is Read-only"));
        }
        if Self::is_protected(path) && self.permissions.run != RunAccess::Full {
            return Err(denied(format!(
                "\"{path}\" configures what runs on the host and needs Full access"
            )));
        }
        Ok(())
    }

    fn need_edit(&self) -> Result<(), Denied> {
        if self.permissions.can_edit_files() {
            Ok(())
        } else {
            Err(denied("Your permission is Read-only"))
        }
    }

    /// Decide a project call from an admitted guest before the host acts on it.
    pub fn check_call(&self, call: &HostCall) -> Result<(), Denied> {
        match call {
            HostCall::Ping | HostCall::Open | HostCall::Detect | HostCall::Watch => Ok(()),
            HostCall::ListDir { path } if path.is_empty() => Ok(()),
            HostCall::ListDir { path }
            | HostCall::ReadFile { path }
            | HostCall::ReadBytes { path }
            | HostCall::ReadOptional { path } => self.check_path(path),
            HostCall::WriteFile { path, .. }
            | HostCall::WriteBytes { path, .. }
            | HostCall::CreateFile { path }
            | HostCall::CreateDir { path }
            | HostCall::Delete { path } => self.check_write_path(path),
            HostCall::Rename { from, to } => {
                self.check_write_path(from)?;
                self.check_write_path(to)
            }
            HostCall::FindFiles { .. } => Ok(()),
            HostCall::FindInFiles(req) => {
                if req.directory.is_empty() {
                    Ok(())
                } else {
                    self.check_path(&req.directory)
                }
            }
            HostCall::Git(q) => self.check_git(q),
            HostCall::GitAction(a) => self.check_git_action(a),
            HostCall::Php(p) => self.check_php(p),
            HostCall::Framework(f) => self.check_framework(f),
            HostCall::Lsp(q) => self.check_lsp(q),
            HostCall::TerminalOpen { cols, rows } => {
                self.need_terminal_full()?;
                if (1..=1000).contains(cols) && (1..=500).contains(rows) {
                    Ok(())
                } else {
                    Err(denied("invalid terminal size"))
                }
            }
            // The host checks that the terminal is the guest's own.
            HostCall::TerminalClose { .. } => self.need_terminal_full(),
            HostCall::RunStart { configuration } => {
                self.need_run_full()?;
                if !configuration.trim().is_empty()
                    && configuration.len() <= 256
                    && !configuration.chars().any(char::is_control)
                {
                    Ok(())
                } else {
                    Err(denied("invalid run configuration name"))
                }
            }
            HostCall::RunStop { .. } => self.need_run_full(),
        }
    }

    fn check_git(&self, q: &GitQuery) -> Result<(), Denied> {
        match q {
            GitQuery::Discover
            | GitQuery::HasHead
            | GitQuery::Head
            | GitQuery::Status
            | GitQuery::Refs
            | GitQuery::Reflog
            | GitQuery::Remotes
            | GitQuery::RemotesVerbose => Ok(()),
            // The host checks the path again against the project folder of the repository.
            GitQuery::Show { rev: r, path } => {
                opt_rev(&Some(r.clone()))?;
                self.check_path(path)
            }
            GitQuery::CommitMeta { rev: r } => rev(r),
            GitQuery::CommitFiles { parent, sha } => {
                opt_rev(parent)?;
                rev(sha)
            }
            GitQuery::Diff { paths, .. }
            | GitQuery::TrackedAmong { paths }
            | GitQuery::LsFiles { paths }
            | GitQuery::LsTreeHead { paths } => pathspecs(paths),
            GitQuery::Blame { path, .. } => self.check_path(path),
            GitQuery::Log {
                rev: r,
                path,
                text,
                author,
                ..
            } => {
                opt_rev(r)?;
                if let Some(p) = path {
                    self.check_path(p)?;
                }
                for s in [text, author].into_iter().flatten() {
                    if s.chars().any(char::is_control) || s.len() > 1024 {
                        return Err(denied("invalid search text"));
                    }
                }
                Ok(())
            }
            GitQuery::HasBranch { name } => rev(name),
        }
    }

    fn check_git_action(&self, a: &GitAction) -> Result<(), Denied> {
        if self.permissions.vcs != VcsAccess::Full {
            return Err(denied(
                "Changing the repository needs Full access to version control",
            ));
        }
        match a {
            GitAction::Stage { paths }
            | GitAction::AddNew { paths }
            | GitAction::Unstage { paths }
            | GitAction::UnstageInitial { paths }
            | GitAction::RestoreFromHead { paths }
            | GitAction::ForgetAdded { paths } => pathspecs(paths),
            GitAction::Commit { paths, .. } => pathspecs(paths),
            GitAction::Checkout { rev: r } | GitAction::Merge { rev: r } => rev(r),
            GitAction::Rebase { onto } => rev(onto),
            GitAction::CheckoutTracking {
                name,
                remote_branch,
            } => {
                rev(name)?;
                rev(remote_branch)
            }
            GitAction::CheckoutNew { name, start } | GitAction::BranchNew { name, start } => {
                rev(name)?;
                opt_rev(start)
            }
            GitAction::BranchDelete { name, .. } => rev(name),
            GitAction::BranchRename { old, new } => {
                rev(old)?;
                rev(new)
            }
            GitAction::Push { set_upstream, .. } => opt_rev(set_upstream),
            GitAction::Pull { .. } | GitAction::Fetch => Ok(()),
            GitAction::FetchRefspec { remote, refspec } => {
                rev(remote)?;
                // A refspec has a colon (`src:dst`) and may start with `+`.
                let r = refspec.strip_prefix('+').unwrap_or(refspec);
                if r.is_empty()
                    || r.starts_with('-')
                    || r.chars().any(|c| c.is_control() || c.is_whitespace())
                {
                    return Err(denied("invalid refspec"));
                }
                Ok(())
            }
        }
    }

    fn check_php(&self, p: &PhpCall) -> Result<(), Denied> {
        match p {
            PhpCall::Index | PhpCall::Search { .. } | PhpCall::LanguagePlaces => Ok(()),
            PhpCall::Structure { path, .. }
            | PhpCall::Hover { path, .. }
            | PhpCall::GenerateTargets { path, .. }
            | PhpCall::Usages { path, .. }
            | PhpCall::Highlights { path, .. }
            | PhpCall::Navigate { path, .. }
            | PhpCall::Definition { path, .. }
            | PhpCall::LineMarkers { path, .. }
            | PhpCall::Hierarchy { path, .. }
            // Intentions are proposals; applying one is an edit the host checks again.
            | PhpCall::Intentions { path, .. }
            | PhpCall::UnresolvedClasses { path, .. }
            | PhpCall::Inspect { path, .. }
            | PhpCall::Complete { path, .. } => self.check_path(path),
            PhpCall::Generate { path, .. } => {
                self.check_path(path)?;
                self.need_edit()
            }
            // Which language server runs, and where, is the host's business.
            PhpCall::LspConfig { .. } | PhpCall::DetectServers { .. } => Err(denied(
                "Language servers run on the host and are not configured by guests",
            )),
            // The include paths are the host's settings.
            PhpCall::Configure { .. } => Err(denied(
                "The PHP include paths are configured on the host",
            )),
            // External libraries live outside the project, which is all a guest sees.
            PhpCall::Libraries | PhpCall::LibraryList { .. } | PhpCall::LibraryRead { .. } => {
                Err(denied("External libraries are not shared with guests"))
            }
        }
    }

    fn check_framework(&self, f: &FrameworkCall) -> Result<(), Denied> {
        match f {
            FrameworkCall::LaravelDetect
            | FrameworkCall::LaravelRoutes { .. }
            | FrameworkCall::LaravelArtisanCommands { .. }
            | FrameworkCall::LaravelOverview => Ok(()),
            FrameworkCall::LaravelLogs { path, .. } => {
                // `storage/logs/<path>`; empty is the newest log.
                self.check_path("storage/logs")?;
                if path.is_empty() {
                    Ok(())
                } else {
                    self.check_path(&format!("storage/logs/{path}"))
                }
            }
            FrameworkCall::LaravelGenerate { id, write, .. } => {
                identifier(id)?;
                if *write {
                    self.need_edit()
                } else {
                    Ok(())
                }
            }
            FrameworkCall::WordPress { query } => {
                use phpforge_fw_wordpress::api::Query as Wp;
                match query {
                    Wp::Complete { path, .. }
                    | Wp::Goto { path, .. }
                    | Wp::Gutter { path, .. }
                    | Wp::Usages { path, .. }
                    | Wp::Inspect { path, .. } => self.check_path(path),
                    Wp::Detect | Wp::Refresh | Wp::Hooks => Ok(()),
                }
            }
            FrameworkCall::Cms { query } => {
                use phpforge_fw_cms::api::Query as Cms;
                match query {
                    Cms::Complete { path, .. }
                    | Cms::Goto { path, .. }
                    | Cms::Gutter { path, .. } => self.check_path(path),
                    Cms::Detect | Cms::Refresh => Ok(()),
                }
            }
            FrameworkCall::LaravelComplete { path, .. }
            | FrameworkCall::LaravelGoto { path, .. }
            | FrameworkCall::LaravelGutter { path, .. }
            | FrameworkCall::Related { path, .. }
            | FrameworkCall::TestTargets { path, .. } => self.check_path(path),
            FrameworkCall::NewTestPlan {
                path,
                composer_path,
                ..
            } => {
                self.check_path(path)?;
                project_setting(composer_path)
            }
            FrameworkCall::Symfony { config, query } => {
                project_setting(&config.app_directory)?;
                project_setting(&config.templates_directory)?;
                project_setting(&config.console_path)?;
                project_setting(&config.container_xml)?;
                identifier(&config.environment)?;
                match query {
                    SymfonyQuery::Complete { path, .. }
                    | SymfonyQuery::Goto { path, .. }
                    | SymfonyQuery::Gutter { path, .. } => self.check_path(path),
                    _ => Ok(()),
                }
            }
            FrameworkCall::Doctrine { config, query } => {
                for d in config
                    .source_directories
                    .iter()
                    .chain(&config.mapping_directories)
                {
                    project_setting(d)?;
                }
                use DoctrineQuery as D;
                match query {
                    D::Complete { path, .. }
                    | D::Goto { path, .. }
                    | D::Diagnostics { path, .. }
                    | D::Highlight { path, .. }
                    | D::Gutter { path, .. } => self.check_path(path),
                    D::Entities { .. } => Ok(()),
                }
            }
        }
    }

    fn check_lsp(&self, q: &LspQuery) -> Result<(), Denied> {
        match q {
            LspQuery::WorkspaceSymbols { .. } => Ok(()),
            LspQuery::Format { path, .. } => {
                self.check_path(path)?;
                self.need_edit()
            }
            LspQuery::Sync { path }
            | LspQuery::Close { path }
            | LspQuery::Hover { path, .. }
            | LspQuery::Completion { path, .. }
            | LspQuery::CompletionResolve { path, .. }
            | LspQuery::Definition { path, .. }
            | LspQuery::References { path, .. }
            | LspQuery::DocumentSymbols { path }
            | LspQuery::SignatureHelp { path, .. } => self.check_path(path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::{Permissions, TerminalAccess};
    use forge_core::search::FindRequest;

    fn policy(p: Permissions) -> Policy {
        Policy::new(p, Excludes::defaults())
    }

    fn write(path: &str) -> HostCall {
        HostCall::WriteFile {
            path: path.into(),
            contents: String::new(),
        }
    }

    #[test]
    fn terminals_and_runs_need_full_access_in_their_area() {
        use crate::permissions::RunAccess;
        use crate::share::{RunId, TerminalId};
        let open = HostCall::TerminalOpen { cols: 80, rows: 24 };
        let close = HostCall::TerminalClose {
            terminal: TerminalId(1),
        };
        let run = HostCall::RunStart {
            configuration: "UserTest".into(),
        };
        let stop = HostCall::RunStop { run: RunId(1) };
        for p in [Permissions::READ_ONLY, Permissions::EDIT_FILES] {
            for call in [&open, &close, &run, &stop] {
                assert!(policy(p).check_call(call).is_err(), "{call:?} with {p:?}");
            }
        }
        let full = policy(Permissions::FULL_ACCESS);
        for call in [&open, &close, &run, &stop] {
            assert!(full.check_call(call).is_ok(), "{call:?}");
        }
        let terminal_only = policy(Permissions {
            terminal: TerminalAccess::Full,
            run: RunAccess::ReadOnly,
            ..Permissions::READ_ONLY
        });
        assert!(terminal_only.check_call(&open).is_ok());
        assert!(terminal_only.check_call(&run).is_err());
        for bad in [
            HostCall::TerminalOpen { cols: 0, rows: 24 },
            HostCall::TerminalOpen {
                cols: 80,
                rows: 9999,
            },
            HostCall::RunStart {
                configuration: " ".into(),
            },
            HostCall::RunStart {
                configuration: "a\nb".into(),
            },
        ] {
            assert!(full.check_call(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn looking_is_allowed_at_every_level() {
        let calls = [
            HostCall::Open,
            HostCall::Detect,
            HostCall::Watch,
            HostCall::ListDir { path: "".into() },
            HostCall::ListDir { path: "app".into() },
            HostCall::ReadFile {
                path: "app/User.php".into(),
            },
            HostCall::FindFiles {
                query: "x".into(),
                limit: 10,
                exact: false,
            },
            HostCall::FindInFiles(FindRequest {
                query: "x".into(),
                ..FindRequest::default()
            }),
            HostCall::Git(GitQuery::Status),
            HostCall::Php(PhpCall::Index),
            HostCall::Framework(FrameworkCall::LaravelDetect),
            HostCall::Lsp(LspQuery::Hover {
                path: "a.php".into(),
                line: 0,
                character: 0,
            }),
            HostCall::Lsp(LspQuery::WorkspaceSymbols { query: "u".into() }),
        ];
        for p in [
            Permissions::READ_ONLY,
            Permissions::EDIT_FILES,
            Permissions::FULL_ACCESS,
        ] {
            for c in &calls {
                assert!(policy(p).check_call(c).is_ok(), "{p:?} {c:?}");
            }
        }
    }

    #[test]
    fn changing_files_needs_files_edit() {
        let calls = [
            write("app/a.php"),
            HostCall::CreateFile {
                path: "app/a.php".into(),
            },
            HostCall::CreateDir {
                path: "app/d".into(),
            },
            HostCall::Rename {
                from: "app/a.php".into(),
                to: "app/b.php".into(),
            },
            HostCall::Delete {
                path: "app/a.php".into(),
            },
            HostCall::Lsp(LspQuery::Format {
                path: "app/a.php".into(),
                tab_size: 4,
                insert_spaces: true,
            }),
            HostCall::Php(PhpCall::Generate {
                path: "app/a.php".into(),
                offset: 0,
                kind: phpforge_lang_php::generate::GenerateKind::Getters,
                options: Default::default(),
                content: None,
            }),
        ];
        for c in &calls {
            assert!(
                policy(Permissions::READ_ONLY).check_call(c).is_err(),
                "{c:?}"
            );
            assert!(
                policy(Permissions::EDIT_FILES).check_call(c).is_ok(),
                "{c:?}"
            );
            assert!(
                policy(Permissions::FULL_ACCESS).check_call(c).is_ok(),
                "{c:?}"
            );
        }
        // The terminal does not matter for files.
        let custom = Permissions {
            terminal: TerminalAccess::Full,
            ..Permissions::READ_ONLY
        };
        assert!(policy(custom).check_call(&write("a.php")).is_err());
    }

    #[test]
    fn git_actions_need_full_version_control_access() {
        let action = HostCall::GitAction(GitAction::Fetch);
        assert!(policy(Permissions::READ_ONLY).check_call(&action).is_err());
        assert!(policy(Permissions::EDIT_FILES).check_call(&action).is_err());
        assert!(policy(Permissions::FULL_ACCESS).check_call(&action).is_ok());
        let only_vcs = Permissions {
            vcs: VcsAccess::Full,
            ..Permissions::READ_ONLY
        };
        assert!(policy(only_vcs).check_call(&action).is_ok());
    }

    #[test]
    fn files_that_configure_what_runs_need_run_full_access() {
        for path in [
            "composer.json",
            "package.json",
            "Makefile",
            "packages/a/composer.json",
            ".forge/settings.toml",
            ".forge/runConfigurations.toml",
            ".vscode/tasks.json",
            ".idea/workspace.xml",
            ".husky/pre-commit",
            "artisan",
            "bin/console",
        ] {
            assert!(Policy::is_protected(path), "{path}");
            assert!(
                policy(Permissions::EDIT_FILES)
                    .check_call(&write(path))
                    .is_err(),
                "{path}"
            );
            assert!(
                policy(Permissions::FULL_ACCESS)
                    .check_call(&write(path))
                    .is_ok(),
                "{path}"
            );
        }
        for path in [
            "app/Makefile.php",
            "src/artisan_helper.php",
            "docs/package.jsonl",
        ] {
            assert!(!Policy::is_protected(path), "{path}");
        }
    }

    #[test]
    fn paths_must_be_plain_visible_project_paths() {
        let p = policy(Permissions::FULL_ACCESS);
        for path in [
            "",
            "/etc/passwd",
            "../x",
            "a/../b",
            "a//b",
            "./a",
            "a\\b",
            "C:/x",
            ".git/config",
            "x/.git/config",
            ".env",
            "docker/.env.local",
            "keys/server.pem",
            ".ENV",
        ] {
            for call in [
                HostCall::ReadFile { path: path.into() },
                write(path),
                HostCall::Delete { path: path.into() },
            ] {
                assert!(p.check_call(&call).is_err(), "{path:?} {call:?}");
            }
        }
        assert!(p.check_call(&HostCall::ListDir { path: "".into() }).is_ok());
        assert!(p
            .check_call(&HostCall::ListDir {
                path: ".git".into()
            })
            .is_err());
    }

    #[test]
    fn git_values_cannot_carry_options_or_pathspec_magic() {
        let p = policy(Permissions::FULL_ACCESS);
        let q = |q| HostCall::Git(q);
        for rev in ["--output=x", "-x", "a:b", "a b", "", "a\nb"] {
            assert!(
                p.check_call(&q(GitQuery::CommitMeta { rev: rev.into() }))
                    .is_err(),
                "{rev:?}"
            );
        }
        assert!(p
            .check_call(&q(GitQuery::CommitMeta {
                rev: "origin/feature-1~2".into()
            }))
            .is_ok());
        for spec in [":(top)x", "../x", "a/../b", ":/x", "a\0b"] {
            assert!(
                p.check_call(&q(GitQuery::Diff {
                    staged: false,
                    against_head: false,
                    paths: vec![spec.into()]
                }))
                .is_err(),
                "{spec:?}"
            );
        }
        assert!(p
            .check_call(&q(GitQuery::Diff {
                staged: false,
                against_head: false,
                paths: vec![".".into(), "app/".into(), "app/User.php".into()]
            }))
            .is_ok());
    }

    #[test]
    fn language_servers_are_not_configured_by_guests() {
        let p = policy(Permissions::FULL_ACCESS);
        assert!(p
            .check_call(&HostCall::Php(PhpCall::LspConfig {
                settings: Default::default()
            }))
            .is_err());
        assert!(p
            .check_call(&HostCall::Php(PhpCall::DetectServers {
                settings: Default::default()
            }))
            .is_err());
    }

    #[test]
    fn framework_settings_name_folders_inside_the_project() {
        let p = policy(Permissions::READ_ONLY);
        let symfony = |app: &str, env: &str| {
            HostCall::Framework(FrameworkCall::Symfony {
                config: phpforge_fw_symfony_config(app, env),
                query: SymfonyQuery::Routes,
            })
        };
        assert!(p.check_call(&symfony("src", "dev")).is_ok());
        assert!(p.check_call(&symfony("/etc", "dev")).is_err());
        assert!(p.check_call(&symfony("../x", "dev")).is_err());
        assert!(p.check_call(&symfony("src", "dev/../x")).is_err());
        assert!(p.check_call(&symfony("src", "")).is_err());
    }

    fn phpforge_fw_symfony_config(app: &str, env: &str) -> phpforge_fw_symfony::Config {
        phpforge_fw_symfony::Config {
            app_directory: app.into(),
            environment: env.into(),
            ..Default::default()
        }
    }
}
