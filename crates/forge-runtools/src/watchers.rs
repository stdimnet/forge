//! File Watchers (Settings | Tools | File Watchers): programs that run when a file of some
//! type changes, such as `sass`, `lessc`, `tsc`, `babel` or `prettier`, with PHPStorm's
//! macros in the arguments, output paths and working directory.
//!
//! Watchers are stored in the project setting `tools.fileWatchers` as an `objects` list
//! (string values, see [`FileWatcher::from_object`]). [`plan`] decides which watchers run
//! for saved or changed files and expands their command lines ("Track only root files"
//! runs the root files that import a changed partial instead of the partial itself);
//! [`problems`] turns their output into Problems with the watcher's output filters
//! (`$FILE_PATH$:$LINE$:$COLUMN$`, as for External Tools).

use std::collections::{BTreeMap, BTreeSet};

use forge_output::OutputFilter;
use serde::{Deserialize, Serialize};

use crate::args::{quote_arg, split_args};
use crate::macros::{Expander, MacroContext};

/// The default scope of new watchers.
pub const DEFAULT_SCOPE: &str = "Project Files";

/// File types a watcher can listen to: (id, label, extensions).
pub const FILE_TYPES: &[(&str, &str, &[&str])] = &[
    ("scss", "SCSS style sheet", &["scss"]),
    ("sass", "Sass style sheet", &["sass"]),
    ("less", "Less style sheet", &["less"]),
    ("css", "Cascading style sheet", &["css"]),
    ("typescript", "TypeScript", &["ts", "mts", "cts"]),
    ("tsx", "TypeScript JSX", &["tsx"]),
    ("javascript", "JavaScript", &["js", "mjs", "cjs", "jsx"]),
    ("html", "HTML", &["html", "htm"]),
    ("vue", "Vue.js", &["vue"]),
    ("json", "JSON", &["json"]),
    ("markdown", "Markdown", &["md", "markdown"]),
    ("any", "Any", &[]),
];

/// When the console of a run is shown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShowConsole {
    Always,
    /// Only when the program fails or writes to stderr (PHPStorm's default "On error").
    #[default]
    Error,
    Never,
}

impl ShowConsole {
    fn parse(v: &str) -> Self {
        match v.trim() {
            "always" => ShowConsole::Always,
            "never" => ShowConsole::Never,
            _ => ShowConsole::Error,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            ShowConsole::Always => "always",
            ShowConsole::Error => "error",
            ShowConsole::Never => "never",
        }
    }
}

/// What caused a watcher run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Trigger {
    /// The user saved the file in Forge.
    Save,
    /// The user stopped typing (watchers with "Auto-save edited files to trigger the
    /// watcher"; the UI saves the file first).
    Change,
    /// The file changed on disk outside Forge (git checkout, another editor).
    External,
}

fn yes() -> bool {
    true
}

/// One File Watcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileWatcher {
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// The template it was made from (`scss`, `less`, `typescript`, `babel`, `prettier`,
    /// `custom`); informational.
    #[serde(default)]
    pub template: String,
    /// A [`FILE_TYPES`] id.
    #[serde(default)]
    pub file_type: String,
    /// A scope name (predefined or named); the app resolves it.
    #[serde(default)]
    pub scope: String,
    pub program: String,
    #[serde(default)]
    pub arguments: String,
    /// Files the program writes, separated by `:` or `;` (refreshed after a run).
    #[serde(default)]
    pub output_paths: String,
    /// Empty = the directory of the file (`$FileDir$`).
    #[serde(default)]
    pub working_directory: String,
    /// `NAME=value` lines.
    #[serde(default)]
    pub environment: String,
    /// "Auto-save edited files to trigger the watcher": run while typing, not only on save.
    #[serde(default)]
    pub auto_save: bool,
    /// "Trigger the watcher on external changes".
    #[serde(default)]
    pub on_external: bool,
    /// "Track only root files": partials and imported files run the files that import them.
    #[serde(default)]
    pub track_only_root: bool,
    #[serde(default)]
    pub show_console: ShowConsole,
    /// Output filters (`$FILE_PATH$:$LINE$:$COLUMN$`), one regular expression each.
    #[serde(default)]
    pub output_filters: Vec<String>,
}

fn flag(map: &BTreeMap<String, String>, key: &str, default: bool) -> bool {
    match map.get(key).map(|v| v.trim()) {
        Some("true") => true,
        Some("false") => false,
        _ => default,
    }
}

fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

impl FileWatcher {
    /// Read a watcher from its settings object (string values; filters one per line).
    pub fn from_object(map: &BTreeMap<String, String>) -> Self {
        let get = |k: &str| map.get(k).cloned().unwrap_or_default();
        Self {
            name: get("name"),
            enabled: flag(map, "enabled", true),
            template: get("template"),
            file_type: get("fileType"),
            scope: get("scope"),
            program: get("program"),
            arguments: get("arguments"),
            output_paths: get("outputPaths"),
            working_directory: get("workingDirectory"),
            environment: get("environment"),
            auto_save: flag(map, "autoSave", false),
            on_external: flag(map, "onExternal", false),
            track_only_root: flag(map, "trackOnlyRoot", false),
            show_console: ShowConsole::parse(&get("showConsole")),
            output_filters: lines(&get("outputFilters")),
        }
    }

    /// The settings object of the watcher.
    pub fn to_object(&self) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        let mut put = |k: &str, v: String| {
            m.insert(k.to_string(), v);
        };
        put("name", self.name.clone());
        put("enabled", self.enabled.to_string());
        put("template", self.template.clone());
        put("fileType", self.file_type.clone());
        put("scope", self.scope.clone());
        put("program", self.program.clone());
        put("arguments", self.arguments.clone());
        put("outputPaths", self.output_paths.clone());
        put("workingDirectory", self.working_directory.clone());
        put("environment", self.environment.clone());
        put("autoSave", self.auto_save.to_string());
        put("onExternal", self.on_external.to_string());
        put("trackOnlyRoot", self.track_only_root.to_string());
        put("showConsole", self.show_console.as_str().to_string());
        put("outputFilters", self.output_filters.join("\n"));
        m
    }

    /// Whether the watcher listens to `path` (by file type; the scope is checked by the
    /// caller).
    pub fn accepts(&self, path: &str) -> bool {
        let ext = extension(path);
        match FILE_TYPES.iter().find(|(id, _, _)| *id == self.file_type) {
            Some((_, _, [])) => true,
            Some((_, _, exts)) => exts.iter().any(|e| e.eq_ignore_ascii_case(ext)),
            None => false,
        }
    }

    /// Whether the watcher runs for this trigger.
    pub fn runs_on(&self, trigger: Trigger) -> bool {
        match trigger {
            Trigger::Save => true,
            Trigger::Change => self.auto_save,
            Trigger::External => self.on_external,
        }
    }
}

/// A template of the "+" menu: a watcher with its defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatcherTemplate {
    pub id: String,
    pub label: String,
    pub watcher: FileWatcher,
}

fn template(
    id: &str,
    label: &str,
    file_type: &str,
    program: &str,
    arguments: &str,
    output_paths: &str,
    working_directory: &str,
    track_only_root: bool,
    filters: &[&str],
) -> WatcherTemplate {
    WatcherTemplate {
        id: id.into(),
        label: label.into(),
        watcher: FileWatcher {
            name: label.into(),
            enabled: true,
            template: id.into(),
            file_type: file_type.into(),
            scope: DEFAULT_SCOPE.into(),
            program: program.into(),
            arguments: arguments.into(),
            output_paths: output_paths.into(),
            working_directory: working_directory.into(),
            environment: String::new(),
            auto_save: false,
            on_external: false,
            track_only_root,
            show_console: ShowConsole::Error,
            output_filters: filters.iter().map(|f| f.to_string()).collect(),
        },
    }
}

/// PHPStorm's watcher templates (Sass/SCSS, Less, TypeScript, Babel, Prettier) and an
/// empty custom one.
pub fn templates() -> Vec<WatcherTemplate> {
    vec![
        template(
            "scss",
            "SCSS",
            "scss",
            "sass",
            "$FileName$:$FileNameWithoutExtension$.css",
            "$FileNameWithoutExtension$.css:$FileNameWithoutExtension$.css.map",
            "$FileDir$",
            true,
            // `  styles.scss 3:5  root stylesheet`
            &["$FILE_PATH$ $LINE$:$COLUMN$"],
        ),
        template(
            "sass",
            "Sass",
            "sass",
            "sass",
            "$FileName$:$FileNameWithoutExtension$.css",
            "$FileNameWithoutExtension$.css:$FileNameWithoutExtension$.css.map",
            "$FileDir$",
            true,
            &["$FILE_PATH$ $LINE$:$COLUMN$"],
        ),
        template(
            "less",
            "Less",
            "less",
            "lessc",
            "--no-color $FileName$ $FileNameWithoutExtension$.css",
            "$FileNameWithoutExtension$.css",
            "$FileDir$",
            true,
            // `ParseError: Unrecognised input in /p/styles.less on line 3, column 5:`
            &["in $FILE_PATH$ on line $LINE$, column $COLUMN$"],
        ),
        template(
            "typescript",
            "TypeScript",
            "typescript",
            "tsc",
            "--sourceMap $FileName$",
            "$FileNameWithoutExtension$.js:$FileNameWithoutExtension$.js.map",
            "$FileDir$",
            false,
            // `app.ts(3,5): error TS2322: …`
            &[r"$FILE_PATH$\($LINE$,$COLUMN$\)"],
        ),
        template(
            "babel",
            "Babel",
            "javascript",
            "babel",
            "$FilePathRelativeToProjectRoot$ --out-dir dist --source-maps",
            "dist/$FileDirRelativeToProjectRoot$/$FileNameWithoutExtension$.js:dist/$FileDirRelativeToProjectRoot$/$FileNameWithoutExtension$.js.map",
            "$ProjectFileDir$",
            false,
            // `SyntaxError: /p/src/app.js: Unexpected token (3:5)`
            &[r"$FILE_PATH$: .*\($LINE$:$COLUMN$\)"],
        ),
        template(
            "prettier",
            "Prettier",
            "javascript",
            "prettier",
            "--write $FilePathRelativeToProjectRoot$",
            "$FilePathRelativeToProjectRoot$",
            "$ProjectFileDir$",
            false,
            // `[error] src/app.js: SyntaxError: Unexpected token (3:5)`
            &[r"\[error\] $FILE_PATH$: .*\($LINE$:$COLUMN$\)"],
        ),
        template("custom", "Custom", "any", "", "", "", "", false, &[]),
    ]
}

/// Problems that keep the Edit Watcher dialog from closing.
pub fn validate(watcher: &FileWatcher, others: &[FileWatcher]) -> Vec<String> {
    let mut out = Vec::new();
    let name = watcher.name.trim();
    if name.is_empty() {
        out.push("Specify the watcher name".to_string());
    } else if others.iter().any(|o| o.name.trim() == name) {
        out.push(format!("Watcher \"{name}\" already exists"));
    }
    if !FILE_TYPES.iter().any(|(id, _, _)| *id == watcher.file_type) {
        out.push("Specify the file type".to_string());
    }
    if watcher.program.trim().is_empty() {
        out.push("Specify the program".to_string());
    }
    for f in &watcher.output_filters {
        if let Err(e) = OutputFilter::compile(f) {
            out.push(e);
        }
    }
    out
}

// ---- Root files ---------------------------------------------------------------------------

fn extension(path: &str) -> &str {
    let name = file_name(path);
    match name.rfind('.') {
        Some(i) if i > 0 => &name[i + 1..],
        _ => "",
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn dir_of(path: &str) -> &str {
    path.rfind('/').map(|i| &path[..i]).unwrap_or("")
}

/// Normalize `a/./b/../c` (project-relative, `/` separators).
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for p in path.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

fn join(dir: &str, rel: &str) -> Option<String> {
    if dir.is_empty() {
        normalize(rel)
    } else {
        normalize(&format!("{dir}/{rel}"))
    }
}

fn is_style(ext: &str) -> bool {
    matches!(ext, "scss" | "sass" | "less" | "css")
}

/// The import specifiers of a file: `@import`/`@use`/`@forward` in style sheets, ES
/// `import … from` / `export … from` / `import()` / `require()` in scripts.
pub fn imports(path: &str, text: &str) -> Vec<String> {
    let ext = extension(path).to_ascii_lowercase();
    let mut out = Vec::new();
    if is_style(&ext) {
        for line in text.lines() {
            let t = line.trim_start();
            let rest = ["@import", "@use", "@forward"]
                .iter()
                .find_map(|k| t.strip_prefix(k));
            let Some(rest) = rest else { continue };
            // `@import "a", "b";` and Less options `@import (reference) "a";`
            for q in quoted(rest) {
                if !q.starts_with("sass:") && !q.starts_with("http") && !q.starts_with("url(") {
                    out.push(q);
                }
            }
        }
    } else {
        for (i, _) in text.match_indices("from") {
            if let Some(q) = quoted(&text[i + 4..]).into_iter().next() {
                let between = text[i + 4..].trim_start();
                if between.starts_with(['\'', '"']) {
                    out.push(q);
                }
            }
        }
        for key in ["import(", "require(", "import "] {
            for (i, _) in text.match_indices(key) {
                let rest = text[i + key.len()..].trim_start();
                if rest.starts_with(['\'', '"']) {
                    if let Some(q) = quoted(rest).into_iter().next() {
                        out.push(q);
                    }
                }
            }
        }
        out.retain(|s| s.starts_with('.'));
    }
    out.sort();
    out.dedup();
    out
}

/// The quoted strings at the start of `text`, up to the end of the statement.
fn quoted(text: &str) -> Vec<String> {
    let end = text.find([';', '\n']).unwrap_or(text.len());
    let stmt = &text[..end];
    let mut out = Vec::new();
    let mut rest = stmt;
    while let Some(i) = rest.find(['\'', '"']) {
        let q = rest.as_bytes()[i] as char;
        let after = &rest[i + 1..];
        match after.find(q) {
            Some(j) => {
                out.push(after[..j].to_string());
                rest = &after[j + 1..];
            }
            None => break,
        }
    }
    out
}

/// The files an import specifier of `from` may mean, most likely first.
fn candidates(from: &str, spec: &str) -> Vec<String> {
    let ext = extension(from).to_ascii_lowercase();
    let Some(base) = join(dir_of(from), spec) else {
        return Vec::new();
    };
    let mut out = vec![base.clone()];
    if is_style(&ext) {
        let (d, n) = (dir_of(&base), file_name(&base));
        let partial = if d.is_empty() {
            format!("_{n}")
        } else {
            format!("{d}/_{n}")
        };
        for e in ["scss", "sass", "less", "css"] {
            out.push(format!("{base}.{e}"));
            out.push(format!("{partial}.{e}"));
            out.push(format!("{base}/_index.{e}"));
            out.push(format!("{base}/index.{e}"));
        }
    } else {
        for e in ["ts", "tsx", "mts", "js", "jsx", "mjs", "cjs"] {
            out.push(format!("{base}.{e}"));
            out.push(format!("{base}/index.{e}"));
        }
    }
    out
}

/// Which files import which: `file → files it imports` over the given sources.
fn graph(files: &BTreeMap<String, String>) -> BTreeMap<String, BTreeSet<String>> {
    let mut g: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (path, text) in files {
        let deps = g.entry(path.clone()).or_default();
        for spec in imports(path, text) {
            if let Some(found) = candidates(path, &spec)
                .into_iter()
                .find(|c| files.contains_key(c))
            {
                if &found != path {
                    deps.insert(found);
                }
            }
        }
    }
    g
}

/// Whether a file name marks a Sass partial (`_variables.scss`), which is never a root.
pub fn is_partial(path: &str) -> bool {
    let ext = extension(path).to_ascii_lowercase();
    matches!(ext.as_str(), "scss" | "sass") && file_name(path).starts_with('_')
}

/// "Track only root files": the root files to run for a changed file. `files` are the
/// project files of the watcher's type with their text (project-relative paths). A file
/// nobody imports (and that is not a Sass partial) is a root and runs itself; an imported
/// file runs every root that imports it, directly or through other files.
pub fn roots_for(changed: &str, files: &BTreeMap<String, String>) -> Vec<String> {
    let g = graph(files);
    let imported: BTreeSet<&String> = g.values().flatten().collect();
    let is_root = |p: &String| !imported.contains(p) && !is_partial(p);
    if !imported.contains(&changed.to_string()) && !is_partial(changed) {
        return vec![changed.to_string()];
    }
    // Reverse edges: who imports whom.
    let mut importers: BTreeMap<&String, Vec<&String>> = BTreeMap::new();
    for (from, deps) in &g {
        for d in deps {
            importers.entry(d).or_default().push(from);
        }
    }
    let start = changed.to_string();
    let mut seen: BTreeSet<&String> = BTreeSet::new();
    let mut stack: Vec<&String> = importers.get(&start).cloned().unwrap_or_default();
    let mut out = BTreeSet::new();
    while let Some(p) = stack.pop() {
        if !seen.insert(p) {
            continue;
        }
        if is_root(p) {
            out.insert(p.clone());
        }
        if let Some(next) = importers.get(p) {
            stack.extend(next.iter().copied());
        }
    }
    out.into_iter().collect()
}

// ---- Planning -----------------------------------------------------------------------------

/// What one watcher run starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatcherRun {
    /// The watcher's name (the Run tab title).
    pub watcher: String,
    /// The project-relative file it runs for.
    pub file: String,
    pub program: String,
    pub args: Vec<String>,
    /// Absolute working directory.
    pub cwd: String,
    pub env: BTreeMap<String, String>,
    /// Expanded output paths (absolute, or relative to `cwd` as written).
    pub outputs: Vec<String>,
    pub output_filters: Vec<String>,
    pub show_console: ShowConsole,
    /// The command line for the console header.
    pub command_line: String,
}

/// The inputs of [`plan`] besides the watchers.
pub struct PlanInput<'a> {
    /// Project-relative paths that were saved or changed.
    pub paths: &'a [String],
    pub trigger: Trigger,
    /// Absolute project root and project name, for the macros.
    pub root: &'a str,
    pub project_name: &'a str,
    /// Whether a path belongs to the watcher's scope (scope name, path).
    pub in_scope: &'a dyn Fn(&str, &str) -> bool,
    /// Project files of a watcher's file type with their text, for "Track only root files"
    /// (called only for such watchers).
    pub sources: &'a dyn Fn(&FileWatcher) -> BTreeMap<String, String>,
    /// A local executable for a bare program name (`node_modules/.bin/sass`), if any.
    pub resolve_program: &'a dyn Fn(&str) -> Option<String>,
}

fn env_lines(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            let k = k.trim();
            (!k.is_empty()).then(|| (k.to_string(), v.trim().to_string()))
        })
        .collect()
}

/// Expand one watcher for one (project-relative) file.
pub fn command(
    watcher: &FileWatcher,
    file: &str,
    root: &str,
    project_name: &str,
    resolve_program: &dyn Fn(&str) -> Option<String>,
) -> WatcherRun {
    let ctx = MacroContext {
        file_path: Some(file.to_string()),
        ..MacroContext::default()
    }
    .with_project(root, project_name);
    let mut e = Expander::new(&ctx);
    let mut program = e.expand(watcher.program.trim());
    if !program.contains(['/', '\\']) {
        if let Some(local) = resolve_program(&program) {
            program = local;
        }
    }
    let args: Vec<String> = split_args(&watcher.arguments)
        .iter()
        .map(|a| e.expand(a))
        .collect();
    let cwd = match watcher.working_directory.trim() {
        "" => e.expand("$FileDir$"),
        wd => e.expand(wd),
    };
    let outputs = watcher
        .output_paths
        .split([':', ';'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| e.expand(p))
        .collect();
    let command_line = std::iter::once(program.as_str())
        .chain(args.iter().map(String::as_str))
        .map(quote_arg)
        .collect::<Vec<_>>()
        .join(" ");
    WatcherRun {
        watcher: watcher.name.clone(),
        file: file.to_string(),
        program,
        args,
        cwd,
        env: env_lines(&watcher.environment),
        outputs,
        output_filters: watcher.output_filters.clone(),
        show_console: watcher.show_console,
        command_line,
    }
}

/// The runs for files that were saved or changed: every enabled watcher whose file type,
/// scope and trigger match, once per (watcher, file). Files a watcher writes itself (its
/// output paths) never trigger it again.
pub fn plan(watchers: &[FileWatcher], input: &PlanInput) -> Vec<WatcherRun> {
    let mut out: Vec<WatcherRun> = Vec::new();
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    for w in watchers.iter().filter(|w| w.enabled) {
        if !w.runs_on(input.trigger) || w.program.trim().is_empty() {
            continue;
        }
        let scope = match w.scope.trim() {
            "" => DEFAULT_SCOPE,
            s => s,
        };
        let mut sources: Option<BTreeMap<String, String>> = None;
        for path in input.paths {
            let path = path.trim_start_matches("./");
            if !w.accepts(path) || !(input.in_scope)(scope, path) {
                continue;
            }
            let files = if w.track_only_root {
                let src = sources.get_or_insert_with(|| (input.sources)(w));
                roots_for(path, src)
            } else {
                vec![path.to_string()]
            };
            for file in files {
                if !(input.in_scope)(scope, &file) || !seen.insert((w.name.clone(), file.clone()))
                {
                    continue;
                }
                let run = command(
                    w,
                    &file,
                    input.root,
                    input.project_name,
                    input.resolve_program,
                );
                out.push(run);
            }
        }
    }
    // A watcher's own outputs (`styles.css` of `styles.scss`) do not trigger other runs.
    let outputs: BTreeSet<String> = out
        .iter()
        .flat_map(|r| {
            r.outputs.iter().map(|o| {
                let abs = if o.starts_with('/') || o.contains(":\\") {
                    o.clone()
                } else {
                    format!("{}/{o}", r.cwd)
                };
                normalize(&abs).unwrap_or(abs)
            })
        })
        .collect();
    let root = input.root.trim_end_matches('/');
    out.retain(|r| {
        let abs = normalize(&format!("{root}/{}", r.file)).unwrap_or_default();
        !outputs.contains(&abs)
    });
    out
}

// ---- Problems -----------------------------------------------------------------------------

/// A problem found in a watcher's output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatcherProblem {
    /// Project-relative when the file is inside the project, else as written (absolute).
    pub path: String,
    /// 1-based, when the filter has `$LINE$` / `$COLUMN$`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    pub message: String,
}

/// The output's problems: every match of an output filter, with the matching line (or,
/// when that line is only the location, the closest error line above it) as the message.
/// `cwd` resolves relative paths; `root` makes paths project-relative.
pub fn problems(output: &str, filters: &[String], cwd: &str, root: &str) -> Vec<WatcherProblem> {
    let (compiled, _) = forge_output::filters::compile_filters(filters);
    let clean: Vec<String> = output.lines().map(strip_ansi).collect();
    let root = root.trim_end_matches(['/', '\\']).replace('\\', "/");
    let cwd = cwd.trim_end_matches(['/', '\\']).replace('\\', "/");
    let mut out: Vec<WatcherProblem> = Vec::new();
    for (i, line) in clean.iter().enumerate() {
        for f in &compiled {
            for m in f.find(line) {
                let raw = m.path.replace('\\', "/");
                let abs = if raw.starts_with('/') || raw.get(1..2) == Some(":") {
                    raw.clone()
                } else if cwd.is_empty() {
                    raw.clone()
                } else {
                    normalize(&format!("{cwd}/{raw}"))
                        .map(|p| format!("/{p}"))
                        .unwrap_or(raw.clone())
                };
                let abs = if abs.starts_with("//") {
                    abs[1..].to_string()
                } else {
                    abs
                };
                let path = abs
                    .strip_prefix(&format!("{root}/"))
                    .map(str::to_string)
                    .unwrap_or(abs.clone());
                let rest = format!("{}{}", &line[..m.start], &line[m.end..]);
                let rest = rest
                    .trim()
                    .trim_matches(|c: char| c.is_whitespace() || ":-)],".contains(c));
                let mut rest = rest.trim();
                for word in [" in", " at", " on"] {
                    rest = rest.strip_suffix(word).unwrap_or(rest).trim_end();
                }
                let message = if is_message(rest) {
                    rest.to_string()
                } else {
                    clean[..i]
                        .iter()
                        .rev()
                        .take(8)
                        .map(|l| l.trim())
                        .find(|l| l.to_ascii_lowercase().contains("error"))
                        .map(str::to_string)
                        .unwrap_or_else(|| line.trim().to_string())
                };
                let p = WatcherProblem {
                    path,
                    line: m.line,
                    column: m.column,
                    message,
                };
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// Whether the rest of a matched line says something (not just `root stylesheet`).
fn is_message(rest: &str) -> bool {
    let words = rest.split_whitespace().count();
    words >= 3 || rest.to_ascii_lowercase().contains("error")
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scss() -> FileWatcher {
        templates()
            .into_iter()
            .find(|t| t.id == "scss")
            .unwrap()
            .watcher
    }

    fn always(_: &str, _: &str) -> bool {
        true
    }

    fn no_sources(_: &FileWatcher) -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    fn no_local(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn settings_objects_round_trip() {
        let mut w = scss();
        w.environment = "NODE_ENV=production".into();
        w.auto_save = true;
        w.show_console = ShowConsole::Never;
        let back = FileWatcher::from_object(&w.to_object());
        assert_eq!(back, w);
        let partial = FileWatcher::from_object(
            &[("name".to_string(), "x".to_string())]
                .into_iter()
                .collect(),
        );
        assert!(partial.enabled);
        assert_eq!(partial.show_console, ShowConsole::Error);
    }

    #[test]
    fn expands_macros_of_the_scss_template() {
        let run = command(
            &scss(),
            "assets/css/app.scss",
            "/home/u/shop",
            "shop",
            &no_local,
        );
        assert_eq!(run.program, "sass");
        assert_eq!(run.args, vec!["app.scss:app.css"]);
        assert_eq!(run.cwd, "/home/u/shop/assets/css");
        assert_eq!(run.outputs, vec!["app.css", "app.css.map"]);
        assert_eq!(run.command_line, "sass app.scss:app.css");

        let babel = templates()
            .into_iter()
            .find(|t| t.id == "babel")
            .unwrap()
            .watcher;
        let local = |p: &str| (p == "babel").then(|| "/home/u/shop/node_modules/.bin/babel".into());
        let run = command(&babel, "src/lib/a.js", "/home/u/shop", "shop", &local);
        assert_eq!(run.program, "/home/u/shop/node_modules/.bin/babel");
        assert_eq!(
            run.args,
            vec!["src/lib/a.js", "--out-dir", "dist", "--source-maps"]
        );
        assert_eq!(run.cwd, "/home/u/shop");
        assert_eq!(run.outputs[0], "dist/src/lib/a.js");
    }

    #[test]
    fn plans_by_type_scope_and_trigger() {
        let mut ts = templates()
            .into_iter()
            .find(|t| t.id == "typescript")
            .unwrap()
            .watcher;
        ts.scope = "src".into();
        let paths = vec![
            "assets/app.scss".to_string(),
            "src/main.ts".to_string(),
            "test/a.ts".to_string(),
            "README.md".to_string(),
        ];
        let in_scope = |scope: &str, path: &str| scope != "src" || path.starts_with("src/");
        let input = PlanInput {
            paths: &paths,
            trigger: Trigger::Save,
            root: "/p",
            project_name: "p",
            in_scope: &in_scope,
            sources: &|_| {
                [("assets/app.scss".to_string(), String::new())]
                    .into_iter()
                    .collect()
            },
            resolve_program: &no_local,
        };
        let runs = plan(&[scss(), ts.clone()], &input);
        let got: Vec<_> = runs
            .iter()
            .map(|r| (r.watcher.as_str(), r.file.as_str()))
            .collect();
        assert_eq!(got, vec![("SCSS", "assets/app.scss"), ("TypeScript", "src/main.ts")]);

        // Typing only triggers watchers with auto-save; disabled watchers never run.
        let change = PlanInput {
            trigger: Trigger::Change,
            ..input
        };
        assert!(plan(&[scss()], &change).is_empty());
        let mut auto = scss();
        auto.auto_save = true;
        assert_eq!(plan(&[auto.clone()], &change).len(), 1);
        auto.enabled = false;
        assert!(plan(&[auto], &change).is_empty());
        let ext = PlanInput {
            trigger: Trigger::External,
            paths: &paths,
            root: "/p",
            project_name: "p",
            in_scope: &always,
            sources: &no_sources,
            resolve_program: &no_local,
        };
        assert!(plan(&[ts], &ext).is_empty());
    }

    #[test]
    fn outputs_of_a_watcher_do_not_trigger_runs() {
        let mut css = templates()
            .into_iter()
            .find(|t| t.id == "custom")
            .unwrap()
            .watcher;
        css.name = "postcss".into();
        css.file_type = "css".into();
        css.program = "postcss".into();
        let paths = vec!["a/app.scss".to_string(), "a/app.css".to_string()];
        let input = PlanInput {
            paths: &paths,
            trigger: Trigger::Save,
            root: "/p",
            project_name: "p",
            in_scope: &always,
            sources: &|_| [("a/app.scss".to_string(), String::new())].into_iter().collect(),
            resolve_program: &no_local,
        };
        let runs = plan(&[scss(), css], &input);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].file, "a/app.scss");
    }

    #[test]
    fn tracks_only_root_files() {
        let files: BTreeMap<String, String> = [
            ("css/app.scss", "@use 'base';\n@import \"components/button\";"),
            ("css/print.scss", "@import 'base';"),
            ("css/_base.scss", "@use 'vars';"),
            ("css/_vars.scss", "$c: red;"),
            ("css/components/_button.scss", ".b { color: red }"),
            ("css/standalone.scss", "body { margin: 0 }"),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        assert_eq!(
            roots_for("css/_vars.scss", &files),
            vec!["css/app.scss", "css/print.scss"]
        );
        assert_eq!(
            roots_for("css/components/_button.scss", &files),
            vec!["css/app.scss"]
        );
        assert_eq!(
            roots_for("css/standalone.scss", &files),
            vec!["css/standalone.scss"]
        );
        // A partial nobody imports runs nothing.
        assert!(roots_for("css/_orphan.scss", &files).is_empty());

        let ts: BTreeMap<String, String> = [
            ("src/main.ts", "import { a } from './util';\nimport x from \"react\";"),
            ("src/util/index.ts", "export * from './math';"),
            ("src/util/math.ts", "export const a = 1;"),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        assert_eq!(roots_for("src/util/math.ts", &ts), vec!["src/main.ts"]);
        assert_eq!(imports("src/main.ts", &ts["src/main.ts"]), vec!["./util"]);
    }

    #[test]
    fn maps_output_to_problems_with_filters() {
        let tsc = "src/app.ts(3,5): error TS2322: Type 'string' is not assignable to type 'number'.\nFound 1 error.";
        let p = problems(
            tsc,
            &[r"$FILE_PATH$\($LINE$,$COLUMN$\)".to_string()],
            "/p",
            "/p",
        );
        assert_eq!(
            p,
            vec![WatcherProblem {
                path: "src/app.ts".into(),
                line: Some(3),
                column: Some(5),
                message: "error TS2322: Type 'string' is not assignable to type 'number'.".into(),
            }]
        );

        let sass = "Error: Undefined variable.\n  ╷\n3 │   color: $primary;\n  │          ^^^^^^^^\n  ╵\n  app.scss 3:10  root stylesheet\n";
        let p = problems(
            sass,
            &["$FILE_PATH$ $LINE$:$COLUMN$".to_string()],
            "/p/assets/css",
            "/p",
        );
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].path, "assets/css/app.scss");
        assert_eq!((p[0].line, p[0].column), (Some(3), Some(10)));
        assert_eq!(p[0].message, "Error: Undefined variable.");

        let less = "\x1b[31mParseError: Unrecognised input in /p/styles/main.less on line 4, column 2:\x1b[0m";
        let p = problems(
            less,
            &["in $FILE_PATH$ on line $LINE$, column $COLUMN$".to_string()],
            "/p/styles",
            "/p",
        );
        assert_eq!(p[0].path, "styles/main.less");
        assert_eq!(p[0].line, Some(4));
        assert_eq!(p[0].message, "ParseError: Unrecognised input");
    }

    #[test]
    fn validates_watchers() {
        let mut w = scss();
        assert!(validate(&w, &[]).is_empty());
        assert_eq!(validate(&w, &[scss()]), vec!["Watcher \"SCSS\" already exists"]);
        w.program.clear();
        w.file_type = "cobol".into();
        w.output_filters = vec!["no path".into()];
        let problems = validate(&w, &[]);
        assert_eq!(problems.len(), 3, "{problems:?}");
    }
}
