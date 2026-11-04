//! Ruby interpreters on this machine, RubyMine's "Ruby SDKs":
//!
//! - version managers: rbenv (`$RBENV_ROOT` or `~/.rbenv/versions/*`), rvm
//!   (`~/.rvm/rubies/*`, `/usr/local/rvm/rubies/*`, with their gem sets), chruby
//!   (`~/.rubies/*`, `/opt/rubies/*`), asdf (`$ASDF_DATA_DIR` or `~/.asdf/installs/ruby/*`),
//!   mise (`$MISE_DATA_DIR` or `~/.local/share/mise/installs/ruby/*`) and frum / rv
//!   (`~/.frum/versions/*`, `~/.local/share/rv/rubies/*`);
//! - Homebrew (`/opt/homebrew/opt/ruby`, `/usr/local/opt/ruby`), RubyInstaller
//!   (`C:\Ruby*`) and the system Ruby (`PATH`, `/usr/bin/ruby`), without version manager
//!   shims;
//! - the project's interpreter: the setting `ruby.interpreter`, else the version its
//!   `.ruby-version` / `.tool-versions` / `mise.toml` / Gemfile asks for, else the
//!   manager's global version, else the newest.
//!
//! Folders give manager versions without running anything; other interpreters are probed
//! (`ruby -e …`, the interpreter only, never project code) through a [`Runner`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rubyforge_lang_ruby::version::{compare, matches, VersionRequest};
use serde::{Deserialize, Serialize};

/// Runs a program and returns its stdout when it exits successfully.
pub type Runner<'a> = dyn Fn(&Path, &[String]) -> Option<String> + 'a;

/// The real runner (stdin closed, no console window on Windows).
pub fn system_runner(program: &Path, args: &[String]) -> Option<String> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    // In Flatpak Ruby runs on the host.
    let mut cmd = forge_hostspawn::host_command(cmd);
    cmd.stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A Ruby interpreter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interpreter {
    /// The `ruby` executable.
    pub path: String,
    /// `3.3.6`, `jruby-9.4.8.0` (empty when unknown).
    pub version: String,
    /// `rbenv`, `rvm`, `chruby`, `asdf`, `mise`, `frum`, `rv`, `homebrew`, `rubyinstaller`,
    /// `system` or `custom` (added in Settings).
    pub manager: String,
    /// `rbenv: 3.3.6` as the settings page lists it.
    pub name: String,
    /// The `bin` folder of the interpreter (its gem executables live there too).
    pub bin_dir: String,
    /// rvm: the gem home of the Ruby (`~/.rvm/gems/ruby-3.3.6`) and its `@global` set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gem_paths: Vec<String>,
}

/// Where to look.
#[derive(Debug, Clone, Default)]
pub struct RubyEnv {
    pub home: Option<PathBuf>,
    pub path: Vec<PathBuf>,
    /// Environment variables the managers use (`RBENV_ROOT`, `ASDF_DATA_DIR`,
    /// `MISE_DATA_DIR`, `rvm_path`, `GEM_HOME`).
    pub vars: BTreeMap<String, String>,
    /// Folders scanned besides the home folder's (`/opt/rubies`, `/usr/local/rvm`), for
    /// tests.
    pub roots: Vec<PathBuf>,
    pub windows: bool,
    /// Forge's managed gems (ruby-lsp), `<data dir>/forge/langservers/ruby`.
    pub managed_dir: Option<PathBuf>,
}

impl RubyEnv {
    pub fn system() -> Self {
        let vars = [
            "RBENV_ROOT",
            "ASDF_DATA_DIR",
            "MISE_DATA_DIR",
            "rvm_path",
            "GEM_HOME",
            "RUBIES",
        ]
        .iter()
        .filter_map(|k| {
            forge_hostspawn::env_var(k)
                .filter(|v| !v.is_empty())
                .map(|v| (k.to_string(), v))
        })
        .collect();
        RubyEnv {
            home: dirs::home_dir(),
            path: forge_hostspawn::sandbox_path_var()
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
            vars,
            roots: if cfg!(windows) {
                Vec::new()
            } else {
                vec![PathBuf::from("/")]
            },
            windows: cfg!(windows),
            managed_dir: crate::lsp::default_dir(),
        }
    }

    fn exe(&self, name: &str) -> String {
        if self.windows {
            format!("{name}.exe")
        } else {
            name.to_string()
        }
    }

    fn var(&self, k: &str) -> Option<PathBuf> {
        self.vars.get(k).map(PathBuf::from)
    }

    fn home_join(&self, rel: &str) -> Option<PathBuf> {
        self.home.as_ref().map(|h| h.join(rel))
    }

    fn root_join(&self, rel: &str) -> Vec<PathBuf> {
        self.roots.iter().map(|r| r.join(rel)).collect()
    }
}

/// Version folders of a manager (`versions/3.3.6`) with a `bin/ruby`.
fn version_dirs(env: &RubyEnv, dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let ruby = e.path().join("bin").join(env.exe("ruby"));
            ruby.is_file().then_some((name, ruby))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn version_of_folder(name: &str) -> String {
    let v = name.strip_prefix("ruby-").unwrap_or(name);
    // rvm: `ruby-3.3.6@gemset` never names a Ruby folder; chruby: `ruby-3.3.6`.
    v.split('@').next().unwrap_or(v).to_string()
}

fn interpreter(manager: &str, version: String, ruby: &Path) -> Interpreter {
    let bin = ruby
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    Interpreter {
        path: ruby.to_string_lossy().into_owned(),
        name: if version.is_empty() {
            format!("{manager}: {}", ruby.display())
        } else {
            format!("{manager}: {version}")
        },
        version,
        manager: manager.into(),
        bin_dir: bin,
        gem_paths: Vec::new(),
    }
}

/// `print [RUBY_VERSION, RUBY_ENGINE, RUBY_ENGINE_VERSION].join(" ")`.
pub const PROBE: &str =
    "print [RUBY_VERSION, RUBY_ENGINE, (defined?(RUBY_ENGINE_VERSION) ? RUBY_ENGINE_VERSION : RUBY_VERSION), Gem.user_dir, Gem.path.join(File::PATH_SEPARATOR)].join(\"\\n\")";

/// What the probe of an interpreter printed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    /// `3.3.6` or `jruby-9.4.8.0`.
    pub version: String,
    /// `~/.local/share/gem/ruby/3.3.0` (`gem install --user-install` target).
    pub user_dir: String,
    /// Gem.path of the interpreter.
    pub gem_path: Vec<String>,
}

pub fn parse_probe(out: &str) -> Option<Probe> {
    let mut lines = out.lines();
    let ruby_version = lines.next()?.trim().to_string();
    let engine = lines.next().unwrap_or("ruby").trim();
    let engine_version = lines.next().unwrap_or("").trim();
    let user_dir = lines.next().unwrap_or("").trim().to_string();
    let sep = if out.contains(';') && !out.contains(':') {
        ';'
    } else {
        ':'
    };
    let gem_path = lines
        .next()
        .map(|l| {
            // Windows paths contain `:` after the drive letter: PATH_SEPARATOR is `;` there.
            let sep = if l.contains(';') { ';' } else { sep };
            l.split(sep)
                .map(str::to_string)
                .filter(|p| !p.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if ruby_version.is_empty() || !ruby_version.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let version = if engine == "ruby" || engine.is_empty() {
        ruby_version
    } else {
        format!("{engine}-{engine_version}")
    };
    Some(Probe {
        version,
        user_dir,
        gem_path,
    })
}

/// Every interpreter found, managers first, without duplicates (by real path).
pub fn interpreters(env: &RubyEnv, run: &Runner) -> Vec<Interpreter> {
    let mut out: Vec<Interpreter> = Vec::new();
    let managed = |manager: &str, dirs: Vec<Option<PathBuf>>, out: &mut Vec<Interpreter>| {
        for d in dirs.into_iter().flatten() {
            for (name, ruby) in version_dirs(env, &d) {
                if name == "system" {
                    continue;
                }
                out.push(interpreter(manager, version_of_folder(&name), &ruby));
            }
        }
    };
    let rbenv_root = env.var("RBENV_ROOT").or_else(|| env.home_join(".rbenv"));
    managed(
        "rbenv",
        vec![rbenv_root.map(|r| r.join("versions"))],
        &mut out,
    );
    let rvm_root = env.var("rvm_path").or_else(|| env.home_join(".rvm"));
    let mut rvm_dirs = vec![rvm_root.as_ref().map(|r| r.join("rubies"))];
    rvm_dirs.extend(env.root_join("usr/local/rvm/rubies").into_iter().map(Some));
    let before = out.len();
    managed("rvm", rvm_dirs, &mut out);
    for i in &mut out[before..] {
        // `~/.rvm/rubies/ruby-3.3.6/bin/ruby` → `~/.rvm/gems/ruby-3.3.6` and `@global`.
        let ruby_dir = Path::new(&i.bin_dir).parent().map(Path::to_path_buf);
        if let (Some(dir), Some(rubies)) =
            (ruby_dir.clone(), ruby_dir.as_ref().and_then(|d| d.parent()))
        {
            if let (Some(name), Some(root)) = (dir.file_name(), rubies.parent()) {
                let gems = root.join("gems");
                let n = name.to_string_lossy();
                i.gem_paths = vec![
                    gems.join(n.as_ref()).to_string_lossy().into_owned(),
                    gems.join(format!("{n}@global"))
                        .to_string_lossy()
                        .into_owned(),
                ];
            }
        }
    }
    let mut chruby = vec![env.home_join(".rubies")];
    chruby.extend(env.root_join("opt/rubies").into_iter().map(Some));
    if let Some(r) = env.var("RUBIES") {
        // chruby's RUBIES lists Ruby folders themselves: scan their parents.
        chruby.push(r.parent().map(Path::to_path_buf));
    }
    managed("chruby", chruby, &mut out);
    let asdf = env.var("ASDF_DATA_DIR").or_else(|| env.home_join(".asdf"));
    managed(
        "asdf",
        vec![asdf.map(|a| a.join("installs/ruby"))],
        &mut out,
    );
    let mise = env
        .var("MISE_DATA_DIR")
        .or_else(|| env.home_join(".local/share/mise"));
    managed(
        "mise",
        vec![mise.map(|m| m.join("installs/ruby"))],
        &mut out,
    );
    managed("frum", vec![env.home_join(".frum/versions")], &mut out);
    managed(
        "rv",
        vec![env.home_join(".local/share/rv/rubies")],
        &mut out,
    );

    // Homebrew, RubyInstaller, the PATH and system folders: probed for their version.
    let mut others: Vec<(&str, PathBuf)> = Vec::new();
    for rel in ["opt/homebrew/opt/ruby/bin", "usr/local/opt/ruby/bin"] {
        for d in env.root_join(rel) {
            others.push(("homebrew", d.join(env.exe("ruby"))));
        }
    }
    if env.windows {
        for root in &env.roots {
            if let Ok(entries) = std::fs::read_dir(root) {
                for e in entries.flatten() {
                    let n = e.file_name().to_string_lossy().to_lowercase();
                    if n.starts_with("ruby") {
                        others.push(("rubyinstaller", e.path().join("bin").join("ruby.exe")));
                    }
                }
            }
        }
    }
    for d in &env.path {
        let s = d.to_string_lossy();
        // Version manager shims and switchers resolve to a managed Ruby listed above.
        if s.contains("shims") || s.contains(".rbenv") || s.contains(".asdf") || s.contains("mise")
        {
            continue;
        }
        others.push(("system", d.join(env.exe("ruby"))));
    }
    for rel in ["usr/bin", "usr/local/bin"] {
        for d in env.root_join(rel) {
            others.push(("system", d.join(env.exe("ruby"))));
        }
    }
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let mut seen: Vec<PathBuf> = out.iter().map(|i| canon(Path::new(&i.path))).collect();
    for (manager, ruby) in others {
        if !ruby.is_file() {
            continue;
        }
        let real = canon(&ruby);
        if seen.contains(&real) {
            continue;
        }
        // A Ruby inside a manager's folder (PATH set by the manager) is listed already.
        if out
            .iter()
            .any(|i| real.starts_with(canon(Path::new(&i.bin_dir))))
        {
            continue;
        }
        seen.push(real.clone());
        let version = run(&ruby, &["-e".to_string(), PROBE.to_string()])
            .and_then(|o| parse_probe(&o))
            .map(|p| p.version)
            .unwrap_or_default();
        let mut i = interpreter(manager, version, &ruby);
        // `/usr/local/bin/ruby` → `/opt/ruby-3.3.6/bin/ruby`: its gem executables are there.
        if let Some(dir) = real.parent().filter(|_| real != ruby) {
            i.bin_dir = dir.to_string_lossy().into_owned();
        }
        out.push(i);
    }
    out
}

/// The manager's global version (`~/.rbenv/version`, asdf / mise `~/.tool-versions`).
pub fn global_version(env: &RubyEnv) -> Option<String> {
    let rbenv_root = env.var("RBENV_ROOT").or_else(|| env.home_join(".rbenv"))?;
    if let Some(v) = std::fs::read_to_string(rbenv_root.join("version"))
        .ok()
        .and_then(|t| rubyforge_lang_ruby::version::parse_ruby_version(&t))
    {
        return Some(v);
    }
    env.home_join(".tool-versions")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| rubyforge_lang_ruby::version::parse_tool_versions(&t))
}

/// Why the project uses its interpreter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chosen {
    pub interpreter: Interpreter,
    /// `setting`, `versionFile`, `global` or `newest`.
    pub reason: String,
    /// The version file and the version it asks for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<VersionRequest>,
}

/// Pick the project's interpreter: the setting (a path or a name/version), the requested
/// version (newest match), the global version, the newest.
pub fn choose(
    list: &[Interpreter],
    setting: &str,
    request: Option<&VersionRequest>,
    global: Option<&str>,
) -> Option<Chosen> {
    let setting = setting.trim();
    if !setting.is_empty() {
        if let Some(i) = list
            .iter()
            .find(|i| i.path == setting || i.name == setting)
            .or_else(|| {
                list.iter()
                    .filter(|i| matches(setting, &i.version))
                    .max_by(|a, b| compare(&a.version, &b.version))
            })
        {
            return Some(Chosen {
                interpreter: i.clone(),
                reason: "setting".into(),
                request: request.cloned(),
            });
        }
        if Path::new(setting).is_absolute() {
            let p = Path::new(setting);
            return Some(Chosen {
                interpreter: interpreter("custom", String::new(), p),
                reason: "setting".into(),
                request: request.cloned(),
            });
        }
    }
    let newest_match = |v: &str| {
        list.iter()
            .filter(|i| matches(v, &i.version))
            .rev()
            .max_by(|a, b| compare(&a.version, &b.version))
    };
    if let Some(r) = request {
        if let Some(i) = newest_match(&r.version) {
            return Some(Chosen {
                interpreter: i.clone(),
                reason: "versionFile".into(),
                request: Some(r.clone()),
            });
        }
    }
    if let Some(i) = global.and_then(newest_match) {
        return Some(Chosen {
            interpreter: i.clone(),
            reason: "global".into(),
            request: request.cloned(),
        });
    }
    list.iter()
        .filter(|i| rubyforge_lang_ruby::version::engine(&i.version) == "ruby")
        .rev()
        .max_by(|a, b| compare(&a.version, &b.version))
        .or_else(|| list.first())
        .map(|i| Chosen {
            interpreter: i.clone(),
            reason: "newest".into(),
            request: request.cloned(),
        })
}

/// Gem executables Forge uses.
pub const TOOLS: [&str; 9] = [
    "gem", "bundle", "rake", "rspec", "rubocop", "rdbg", "irb", "ruby-lsp", "erb",
];

/// Where a gem executable of `interp` is: its `bin` folder, its user gem folder, rvm's gem
/// sets, `GEM_HOME/bin`, Forge's managed gems (ruby-lsp), then the PATH (outside other
/// managers' shims).
pub fn find_tool(
    env: &RubyEnv,
    interp: Option<&Interpreter>,
    user_dir: Option<&str>,
    tool: &str,
) -> Option<String> {
    let file = env.exe(tool);
    let bat = format!("{tool}.bat");
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(i) = interp {
        dirs.push(PathBuf::from(&i.bin_dir));
        dirs.extend(i.gem_paths.iter().map(|g| Path::new(g).join("bin")));
    }
    if let Some(u) = user_dir.filter(|u| !u.is_empty()) {
        dirs.push(Path::new(u).join("bin"));
    }
    if let Some(g) = env.var("GEM_HOME") {
        dirs.push(g.join("bin"));
    }
    if tool == "ruby-lsp" {
        if let Some(m) = &env.managed_dir {
            if let Some(i) = interp {
                dirs.push(crate::lsp::install_dir(m, &i.version).join("bin"));
            }
        }
    }
    dirs.extend(
        env.path
            .iter()
            .filter(|d| !d.to_string_lossy().contains("shims"))
            .cloned(),
    );
    dirs.into_iter()
        .flat_map(|d| [d.join(&file), d.join(&bat)])
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
}

/// The status of Ruby for the project (Settings | Ruby SDK, the status bar, tool windows).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RubyStatus {
    pub interpreters: Vec<Interpreter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chosen: Option<Chosen>,
    /// The project's version file (also when no interpreter matches it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<VersionRequest>,
    /// Gem executables of the chosen interpreter (`tool` → path).
    pub tools: BTreeMap<String, String>,
    /// The chosen interpreter's probe (user gem folder, gem path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe: Option<Probe>,
    /// Version managers found (`rbenv`, `rvm`, …).
    pub managers: Vec<String>,
}

/// Detect interpreters and the project's choice.
pub fn detect(
    env: &RubyEnv,
    setting: &str,
    request: Option<VersionRequest>,
    run: &Runner,
) -> RubyStatus {
    let list = interpreters(env, run);
    let global = global_version(env);
    let chosen = choose(&list, setting, request.as_ref(), global.as_deref());
    let probe = chosen
        .as_ref()
        .and_then(|c| {
            run(
                Path::new(&c.interpreter.path),
                &["-e".to_string(), PROBE.to_string()],
            )
        })
        .and_then(|o| parse_probe(&o));
    let mut tools = BTreeMap::new();
    for t in TOOLS {
        if let Some(p) = find_tool(
            env,
            chosen.as_ref().map(|c| &c.interpreter),
            probe.as_ref().map(|p| p.user_dir.as_str()),
            t,
        ) {
            tools.insert(t.to_string(), p);
        }
    }
    let mut managers: Vec<String> = list.iter().map(|i| i.manager.clone()).collect();
    managers.dedup();
    managers.sort();
    managers.dedup();
    RubyStatus {
        interpreters: list,
        chosen,
        request,
        tools,
        probe,
        managers,
    }
}

/// The environment of Ruby commands: `PATH` with the interpreter's `bin` (and user gem
/// `bin`) first, rvm's `GEM_HOME` / `GEM_PATH`.
pub fn command_env(
    status: &RubyStatus,
    base_path: &str,
    windows: bool,
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    let Some(c) = &status.chosen else {
        return env;
    };
    let sep = if windows { ";" } else { ":" };
    let mut parts: Vec<String> = vec![c.interpreter.bin_dir.clone()];
    if let Some(p) = &status.probe {
        if !p.user_dir.is_empty() {
            parts.push(format!(
                "{}{}bin",
                p.user_dir,
                if windows { "\\" } else { "/" }
            ));
        }
    }
    parts.extend(c.interpreter.gem_paths.iter().map(|g| format!("{g}/bin")));
    if !base_path.is_empty() {
        parts.push(base_path.to_string());
    }
    env.insert("PATH".into(), parts.join(sep));
    if let Some(home) = c.interpreter.gem_paths.first() {
        env.insert("GEM_HOME".into(), home.clone());
        env.insert("GEM_PATH".into(), c.interpreter.gem_paths.join(sep));
    }
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ruby_at(dir: &Path) -> PathBuf {
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let ruby = bin.join("ruby");
        std::fs::write(&ruby, "#!/bin/sh\n").unwrap();
        ruby
    }

    fn fake_env(home: &Path) -> RubyEnv {
        RubyEnv {
            home: Some(home.join("home")),
            roots: vec![home.join("root")],
            path: vec![home.join("root/usr/bin"), home.join("home/.rbenv/shims")],
            ..RubyEnv::default()
        }
    }

    fn probe_runner(program: &Path, _args: &[String]) -> Option<String> {
        program
            .to_string_lossy()
            .contains("usr/bin")
            .then(|| "3.0.2\nruby\n3.0.2\n/home/anna/.local/share/gem/ruby/3.0.0\n/var/lib/gems/3.0.0:/usr/share/rubygems-integration/all\n".to_string())
    }

    #[test]
    fn managers_homebrew_and_system_rubies() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        ruby_at(&t.join("home/.rbenv/versions/3.3.6"));
        ruby_at(&t.join("home/.rbenv/versions/3.2.4"));
        ruby_at(&t.join("home/.rvm/rubies/ruby-3.1.6"));
        ruby_at(&t.join("home/.rubies/ruby-3.4.1"));
        ruby_at(&t.join("home/.asdf/installs/ruby/jruby-9.4.8.0"));
        ruby_at(&t.join("home/.local/share/mise/installs/ruby/3.3.5"));
        ruby_at(&t.join("root/opt/rubies/ruby-3.4.0"));
        ruby_at(&t.join("root/usr"));
        std::fs::create_dir_all(t.join("home/.rbenv/shims")).unwrap();
        std::fs::write(t.join("home/.rbenv/shims/ruby"), "#!/bin/sh\n").unwrap();
        let env = fake_env(t);
        let list = interpreters(&env, &probe_runner);
        let names: Vec<_> = list.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "rbenv: 3.2.4",
                "rbenv: 3.3.6",
                "rvm: 3.1.6",
                "chruby: 3.4.1",
                "chruby: 3.4.0",
                "asdf: jruby-9.4.8.0",
                "mise: 3.3.5",
                "system: 3.0.2",
            ]
        );
        let rvm = &list[2];
        assert!(
            rvm.gem_paths[0].ends_with(".rvm/gems/ruby-3.1.6"),
            "{:?}",
            rvm.gem_paths
        );
        assert!(rvm.gem_paths[1].ends_with("ruby-3.1.6@global"));

        // The project's .ruby-version wins over the global version and the newest.
        std::fs::write(t.join("home/.rbenv/version"), "3.2.4\n").unwrap();
        let req = VersionRequest {
            version: "3.3".into(),
            file: ".ruby-version".into(),
        };
        let s = detect(&env, "", Some(req.clone()), &probe_runner);
        let c = s.chosen.as_ref().unwrap();
        assert_eq!(
            (c.interpreter.name.as_str(), c.reason.as_str()),
            ("rbenv: 3.3.6", "versionFile")
        );
        let s = detect(&env, "", None, &probe_runner);
        assert_eq!(s.chosen.unwrap().reason, "global");
        std::fs::remove_file(t.join("home/.rbenv/version")).unwrap();
        let c = choose(&list, "", None, None).unwrap();
        assert_eq!(
            (c.interpreter.version.as_str(), c.reason.as_str()),
            ("3.4.1", "newest")
        );
        let c = choose(&list, "mise: 3.3.5", Some(&req), None).unwrap();
        assert_eq!(
            (c.interpreter.manager.as_str(), c.reason.as_str()),
            ("mise", "setting")
        );
        let c = choose(&list, "/opt/custom/bin/ruby", None, None).unwrap();
        assert_eq!(c.interpreter.manager, "custom");
        assert!(s.managers.contains(&"rbenv".to_string()));
    }

    #[test]
    fn tools_and_environment() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        let ruby = ruby_at(&t.join("home/.rbenv/versions/3.3.6"));
        let bin = ruby.parent().unwrap();
        for tool in ["rake", "rspec", "bundle"] {
            std::fs::write(bin.join(tool), "").unwrap();
        }
        let user = t.join("home/.gem/ruby/3.3.0");
        std::fs::create_dir_all(user.join("bin")).unwrap();
        std::fs::write(user.join("bin/rubocop"), "").unwrap();
        let env = fake_env(t);
        let runner =
            |_: &Path, _: &[String]| Some(format!("3.3.6\nruby\n3.3.6\n{}\n/x", user.display()));
        let s = detect(&env, "", None, &runner);
        assert!(s.tools["rspec"].ends_with("3.3.6/bin/rspec"));
        assert!(s.tools["rubocop"].ends_with(".gem/ruby/3.3.0/bin/rubocop"));
        assert!(!s.tools.contains_key("rdbg"));
        let e = command_env(&s, "/usr/bin", false);
        let path = &e["PATH"];
        assert!(path.starts_with(&bin.to_string_lossy().into_owned()));
        assert!(path.ends_with(":/usr/bin"));
        assert!(!e.contains_key("GEM_HOME"));
    }

    #[test]
    fn probes() {
        let p = parse_probe("9.4.8.0\njruby\n9.4.8.0\n/u\n/a:/b").unwrap();
        assert_eq!(p.version, "jruby-9.4.8.0");
        assert_eq!(p.gem_path, ["/a", "/b"]);
        let w = parse_probe("3.3.6\nruby\n3.3.6\nC:/Users/a/.local/share/gem/ruby/3.3.0\nC:/Ruby33/lib/ruby/gems/3.3.0;C:/Users/a/gems").unwrap();
        assert_eq!(w.gem_path.len(), 2);
        assert!(parse_probe("ruby: command not found").is_none());
    }
}
