//! Laravel, Symfony and Doctrine analysis of a project, run where its files are.
//!
//! The analysis reads hundreds of project files, so it never goes file by file over the
//! network: [`LocalFrameworks`] scans and caches next to the files (in the desktop app for a
//! local project, in the Forge agent for a remote one) and answers [`FrameworkCall`]s.
//! Editor requests (completion, Go to Declaration, gutter marks) carry the buffer, so a
//! remote project needs one round trip per request, like a language server.
//!
//! Replies are typed per call (see the comments on [`FrameworkCall`]); the app decodes them
//! with [`framework_call`]. Offsets are UTF-16 (JS strings) as in the IPC commands.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_core::{ForgeError, Result, Workspace};
use forge_process::CommandLine;
use phpforge_fw_laravel::artisan::{self, ArtisanCommand};
use phpforge_fw_laravel::{Laravel, LaravelInfo};
use phpforge_fw_symfony::{console, Config as SymfonyConfig, Snapshot as SymfonySnapshot};
use phpforge_lang_php::offsets::{byte_to_utf16, utf16_to_byte};
use phpforge_orm_doctrine::php::TreeSitter;
use phpforge_orm_doctrine::{Config as DoctrineConfig, Snapshot as DoctrineSnapshot};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::protocol::{decode_value, encode_value};
use crate::Backend;

/// How often the Laravel files are checked for changes at most.
const LARAVEL_RECHECK: Duration = Duration::from_secs(2);
/// How long a Symfony or Doctrine snapshot is reused before files are scanned again.
const SNAPSHOT_TTL: Duration = Duration::from_secs(10);
/// `php artisan route:list --json` and `list --format=json` may boot a large application.
const ARTISAN_TIMEOUT: Duration = Duration::from_secs(20);

/// A framework request. The reply type of each call is noted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FrameworkCall {
    /// -> [`LaravelInfo`] (`detected: false` for other projects)
    LaravelDetect,
    /// `source`: `static`, `artisan` or `auto` (artisan when it can run) -> [`LaravelRoutes`]
    LaravelRoutes { source: String, php: String },
    /// -> `Option<phpforge_fw_laravel::Completions>`
    LaravelComplete {
        path: String,
        offset: usize,
        content: Option<String>,
    },
    /// -> `Vec<phpforge_fw_laravel::Location>`
    LaravelGoto {
        path: String,
        offset: usize,
        content: Option<String>,
    },
    /// -> `Vec<phpforge_fw_laravel::GutterMark>`
    LaravelGutter {
        path: String,
        content: Option<String>,
    },
    /// -> [`ArtisanCommands`]
    LaravelArtisanCommands { php: String },
    Symfony {
        config: SymfonyConfig,
        query: SymfonyQuery,
    },
    Doctrine {
        config: DoctrineConfig,
        query: DoctrineQuery,
    },
}

/// Symfony requests on the project snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SymfonyQuery {
    /// -> `Option<SymfonyInfo>` (`None` for other projects)
    Detect,
    /// Scan again -> `SymfonyInfo`
    Refresh,
    /// -> `Vec<phpforge_fw_symfony::Route>`
    Routes,
    /// -> `Vec<Service>`
    Services,
    /// -> `Vec<Parameter>`
    Parameters,
    /// -> `Vec<Template>`
    Templates,
    /// Application commands first -> `Vec<ConsoleCommand>`
    ConsoleCommands,
    /// -> `Option<CompletionList>`
    Complete {
        path: String,
        content: String,
        offset: usize,
    },
    /// -> `Vec<phpforge_fw_symfony::Location>`
    Goto {
        path: String,
        content: String,
        offset: usize,
    },
    /// -> `Vec<phpforge_fw_symfony::gutter::GutterMark>`
    Gutter { path: String, content: String },
}

/// Doctrine requests on the entity mapping snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DoctrineQuery {
    /// -> `Vec<Entity>`
    Entities { refresh: bool },
    /// -> `Option<DqlCompletion>`
    Complete {
        path: String,
        content: String,
        offset: usize,
    },
    /// -> `Vec<phpforge_orm_doctrine::Location>`
    Goto {
        path: String,
        content: String,
        offset: usize,
    },
    /// -> `Vec<SourceDiagnostic>`
    Diagnostics { path: String, content: String },
    /// -> `SourceHighlight`
    Highlight { path: String, content: String },
    /// -> `Vec<phpforge_orm_doctrine::php::GutterMark>`
    Gutter { path: String, content: String },
}

/// Reply to [`FrameworkCall::LaravelRoutes`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaravelRoutes {
    pub routes: Vec<phpforge_fw_laravel::Route>,
    /// `static` or `artisan`.
    pub source: String,
}

/// Reply to [`FrameworkCall::LaravelArtisanCommands`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtisanCommands {
    pub commands: Vec<ArtisanCommand>,
    /// `false` when artisan could not run and the built-in list is shown.
    pub from_project: bool,
}

/// Send `call` to the backend and decode the reply.
pub fn framework_call<T: DeserializeOwned>(
    backend: &dyn Backend,
    call: FrameworkCall,
) -> Result<T> {
    decode_value(&backend.framework(call)?)
}

/// `php artisan <line>` in the project root (`line` may start with `php artisan`).
pub fn artisan_command(root: &Path, php: &str, line: &str) -> CommandLine {
    let mut cmd = CommandLine::new(php)
        .arg("artisan")
        .args(artisan::split_command_line(line));
    cmd.cwd = root.to_path_buf();
    cmd.env.insert("NO_COLOR".into(), "1".into());
    cmd
}

/// `php bin/console <command> <args>` in the project root for the configured environment.
pub fn symfony_console_command(
    root: &Path,
    php: &str,
    config: &SymfonyConfig,
    command: &str,
    args: &str,
) -> CommandLine {
    let argv = console::command_line(php, &config.console_path, command, args);
    let mut cmd = CommandLine::new(&argv[0]).args(argv[1..].iter().cloned());
    cmd.cwd = root.to_path_buf();
    cmd.env.insert("APP_ENV".into(), config.environment.clone());
    cmd.env.insert("COLUMNS".into(), "120".into());
    cmd
}

struct CachedLaravel {
    laravel: Laravel,
    checked: Instant,
}

struct Cached<T> {
    at: Instant,
    /// The settings the snapshot was made with.
    key: String,
    snapshot: Arc<T>,
}

impl<T> Cached<T> {
    fn fresh(&self, key: &str) -> Option<Arc<T>> {
        (self.at.elapsed() < SNAPSHOT_TTL && self.key == key).then(|| self.snapshot.clone())
    }
}

/// Framework analysis of a folder on this machine, cached per project.
pub struct LocalFrameworks {
    workspace: Workspace,
    laravel: Mutex<Option<CachedLaravel>>,
    symfony: Mutex<Option<Cached<SymfonySnapshot>>>,
    doctrine: Mutex<Option<Cached<DoctrineSnapshot>>>,
}

fn err(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

fn reply<T: Serialize>(value: T) -> Result<Vec<u8>> {
    encode_value(&value)
}

fn not_detected() -> LaravelInfo {
    LaravelInfo {
        detected: false,
        version: None,
        routes: 0,
        views: 0,
        components: 0,
        models: 0,
        config_keys: 0,
        translations: 0,
        directives: Vec::new(),
    }
}

impl LocalFrameworks {
    pub fn new(workspace: Workspace) -> Self {
        Self {
            workspace,
            laravel: Mutex::new(None),
            symfony: Mutex::new(None),
            doctrine: Mutex::new(None),
        }
    }

    fn root(&self) -> PathBuf {
        self.workspace.root().to_path_buf()
    }

    fn source(&self, path: &str, content: Option<String>) -> Result<String> {
        match content {
            Some(c) => Ok(c),
            None => self.workspace.read_file(path),
        }
    }

    /// Run `f` with the up-to-date Laravel analysis (rebuilt when its files changed).
    fn with_laravel<T>(&self, f: impl FnOnce(&mut Laravel) -> T) -> Result<T> {
        let root = self.root();
        let mut guard = self.laravel.lock().map_err(err)?;
        let stale = match guard.as_mut() {
            Some(c) if c.checked.elapsed() > LARAVEL_RECHECK => {
                c.checked = Instant::now();
                c.laravel.fingerprint != phpforge_fw_laravel::fingerprint(&root)
            }
            Some(_) => false,
            None => true,
        };
        if stale {
            *guard = Some(CachedLaravel {
                laravel: Laravel::load(&root),
                checked: Instant::now(),
            });
        }
        Ok(f(&mut guard.as_mut().expect("loaded above").laravel))
    }

    fn symfony(&self, config: &SymfonyConfig, force: bool) -> Result<Arc<SymfonySnapshot>> {
        let key = format!("{config:?}");
        let mut guard = self.symfony.lock().map_err(err)?;
        if let Some(s) = guard
            .as_ref()
            .filter(|_| !force)
            .and_then(|c| c.fresh(&key))
        {
            return Ok(s);
        }
        let snapshot = Arc::new(phpforge_fw_symfony::scan(&self.root(), config));
        *guard = Some(Cached {
            at: Instant::now(),
            key,
            snapshot: snapshot.clone(),
        });
        Ok(snapshot)
    }

    fn doctrine(&self, config: &DoctrineConfig, force: bool) -> Result<Arc<DoctrineSnapshot>> {
        let key = format!("{config:?}");
        let mut guard = self.doctrine.lock().map_err(err)?;
        if let Some(s) = guard
            .as_ref()
            .filter(|_| !force)
            .and_then(|c| c.fresh(&key))
        {
            return Ok(s);
        }
        let snapshot = Arc::new(phpforge_orm_doctrine::scan(&self.root(), config));
        *guard = Some(Cached {
            at: Instant::now(),
            key,
            snapshot: snapshot.clone(),
        });
        Ok(snapshot)
    }

    /// Answer `call`; the reply is MessagePack encoded like an agent reply.
    pub fn call(&self, call: FrameworkCall) -> Result<Vec<u8>> {
        let root = self.root();
        match call {
            FrameworkCall::LaravelDetect => {
                if !phpforge_fw_laravel::is_laravel(&root) {
                    return reply(not_detected());
                }
                reply(self.with_laravel(|l| l.info())?)
            }
            FrameworkCall::LaravelRoutes { source, php } => {
                let json =
                    if source == "artisan" || (source == "auto" && artisan::is_runnable(&root)) {
                        artisan::capture(&root, &php, &["route:list", "--json"], ARTISAN_TIMEOUT)
                    } else {
                        None
                    };
                reply(self.with_laravel(|l| {
                    if let Some(json) = json {
                        l.use_route_list_json(&json);
                    }
                    LaravelRoutes {
                        routes: l.routes.clone(),
                        source: l.routes_source.clone(),
                    }
                })?)
            }
            FrameworkCall::LaravelComplete {
                path,
                offset,
                content,
            } => {
                let text = self.source(&path, content)?;
                let byte = utf16_to_byte(&text, offset);
                let result = self.with_laravel(|l| l.complete(&path, &text, byte))?;
                reply(result.map(|mut c| {
                    c.from = byte_to_utf16(&text, c.from);
                    c.to = byte_to_utf16(&text, c.to);
                    c
                }))
            }
            FrameworkCall::LaravelGoto {
                path,
                offset,
                content,
            } => {
                let text = self.source(&path, content)?;
                let byte = utf16_to_byte(&text, offset);
                reply(self.with_laravel(|l| l.goto(&path, &text, byte))?)
            }
            FrameworkCall::LaravelGutter { path, content } => {
                let text = self.source(&path, content)?;
                reply(self.with_laravel(|l| l.gutter(&path, &text))?)
            }
            FrameworkCall::LaravelArtisanCommands { php } => {
                let (commands, from_project) = artisan::list_commands(&root, &php);
                reply(ArtisanCommands {
                    commands,
                    from_project,
                })
            }
            FrameworkCall::Symfony { config, query } => self.symfony_query(&root, &config, query),
            FrameworkCall::Doctrine { config, query } => self.doctrine_query(&config, query),
        }
    }

    fn symfony_query(
        &self,
        root: &Path,
        config: &SymfonyConfig,
        query: SymfonyQuery,
    ) -> Result<Vec<u8>> {
        use phpforge_fw_symfony::{complete, gutter};
        match query {
            SymfonyQuery::Detect => {
                if !phpforge_fw_symfony::is_symfony_project(root) {
                    return reply(None::<phpforge_fw_symfony::SymfonyInfo>);
                }
                reply(Some(self.symfony(config, false)?.info.clone()))
            }
            SymfonyQuery::Refresh => reply(&self.symfony(config, true)?.info),
            SymfonyQuery::Routes => reply(&self.symfony(config, false)?.routes),
            SymfonyQuery::Services => reply(&self.symfony(config, false)?.services),
            SymfonyQuery::Parameters => reply(&self.symfony(config, false)?.parameters),
            SymfonyQuery::Templates => reply(&self.symfony(config, false)?.templates),
            SymfonyQuery::ConsoleCommands => reply(console::all_commands(
                &self.symfony(config, false)?.commands,
            )),
            SymfonyQuery::Complete {
                path,
                content,
                offset,
            } => {
                let snap = self.symfony(config, false)?;
                let byte = utf16_to_byte(&content, offset);
                reply(
                    complete::complete(&snap, &path, &content, byte).map(|mut list| {
                        list.from = byte_to_utf16(&content, list.from);
                        list
                    }),
                )
            }
            SymfonyQuery::Goto {
                path,
                content,
                offset,
            } => {
                let snap = self.symfony(config, false)?;
                reply(complete::goto(
                    &snap,
                    &path,
                    &content,
                    utf16_to_byte(&content, offset),
                ))
            }
            SymfonyQuery::Gutter { path, content } => {
                let snap = self.symfony(config, false)?;
                reply(gutter::marks(&snap, &path, &content))
            }
        }
    }

    fn doctrine_query(&self, config: &DoctrineConfig, query: DoctrineQuery) -> Result<Vec<u8>> {
        use phpforge_dql::editor;
        match query {
            DoctrineQuery::Entities { refresh } => reply(&self.doctrine(config, refresh)?.entities),
            DoctrineQuery::Complete {
                path,
                content,
                offset,
            } => {
                let snap = self.doctrine(config, false)?;
                reply(editor::complete(
                    &TreeSitter,
                    &snap,
                    &path,
                    &content,
                    offset,
                ))
            }
            DoctrineQuery::Goto {
                path,
                content,
                offset,
            } => {
                let snap = self.doctrine(config, false)?;
                reply(editor::goto(&TreeSitter, &snap, &path, &content, offset))
            }
            DoctrineQuery::Diagnostics { path, content } => {
                let snap = self.doctrine(config, false)?;
                reply(editor::diagnostics(&TreeSitter, &snap, &path, &content))
            }
            DoctrineQuery::Highlight { path, content } => {
                let snap = self.doctrine(config, false)?;
                reply(editor::highlight(&TreeSitter, &snap, &path, &content))
            }
            DoctrineQuery::Gutter { path, content } => {
                let snap = self.doctrine(config, false)?;
                reply(phpforge_orm_doctrine::php::gutter(&snap, &path, &content))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn laravel_project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::create_dir_all(p.join("routes")).unwrap();
        std::fs::create_dir_all(p.join("app/Http/Controllers")).unwrap();
        std::fs::write(
            p.join("composer.json"),
            r#"{"require":{"laravel/framework":"^12.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        )
        .unwrap();
        std::fs::write(
            p.join("routes/web.php"),
            "<?php\nuse App\\Http\\Controllers\\OrderController;\nRoute::get('/orders', [OrderController::class, 'index'])->name('orders.index');\n",
        )
        .unwrap();
        std::fs::write(
            p.join("app/Http/Controllers/OrderController.php"),
            "<?php\nnamespace App\\Http\\Controllers;\nclass OrderController {\n    public function index() {}\n}\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn laravel_routes_and_completion_are_answered_from_the_cache() {
        let dir = laravel_project();
        let fw = LocalFrameworks::new(Workspace::open(dir.path()).unwrap());
        let info: LaravelInfo =
            decode_value(&fw.call(FrameworkCall::LaravelDetect).unwrap()).unwrap();
        assert!(info.detected);
        assert_eq!(info.version.as_deref(), Some("12.0"));
        let routes: LaravelRoutes = decode_value(
            &fw.call(FrameworkCall::LaravelRoutes {
                source: "auto".into(),
                php: "php".into(),
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(routes.source, "static");
        assert_eq!(routes.routes[0].uri, "orders");
        assert_eq!(routes.routes[0].name.as_deref(), Some("orders.index"));

        let content = "<?php route('or');".to_string();
        let offset = content.find("or'").unwrap() + 2;
        let completions: Option<phpforge_fw_laravel::Completions> = decode_value(
            &fw.call(FrameworkCall::LaravelComplete {
                path: "app/x.php".into(),
                offset,
                content: Some(content),
            })
            .unwrap(),
        )
        .unwrap();
        let labels: Vec<String> = completions
            .unwrap()
            .items
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert!(labels.contains(&"orders.index".to_string()), "{labels:?}");
    }

    #[test]
    fn other_projects_are_not_laravel_or_symfony() {
        let dir = tempfile::tempdir().unwrap();
        let fw = LocalFrameworks::new(Workspace::open(dir.path()).unwrap());
        let info: LaravelInfo =
            decode_value(&fw.call(FrameworkCall::LaravelDetect).unwrap()).unwrap();
        assert!(!info.detected);
        let sf: Option<phpforge_fw_symfony::SymfonyInfo> = decode_value(
            &fw.call(FrameworkCall::Symfony {
                config: SymfonyConfig::default(),
                query: SymfonyQuery::Detect,
            })
            .unwrap(),
        )
        .unwrap();
        assert!(sf.is_none());
    }

    #[test]
    fn command_lines_run_in_the_project_root() {
        let cmd = artisan_command(
            Path::new("/srv/shop"),
            "php8.3",
            "php artisan migrate --force",
        );
        assert_eq!(cmd.program, "php8.3");
        assert_eq!(cmd.args, vec!["artisan", "migrate", "--force"]);
        assert_eq!(cmd.cwd, PathBuf::from("/srv/shop"));
        assert_eq!(cmd.env["NO_COLOR"], "1");
        let sf = symfony_console_command(
            Path::new("/srv/app"),
            "php",
            &SymfonyConfig::default(),
            "cache:clear",
            "--no-warmup",
        );
        assert_eq!(sf.args, vec!["bin/console", "cache:clear", "--no-warmup"]);
        assert_eq!(sf.env["APP_ENV"], "dev");
    }
}
