//! PHPStorm's macros for External Tools (`$FilePath$`, `$ProjectFileDir$`, `$Prompt$`, …):
//! the list with descriptions and previews shown by "Insert Macro…", and expansion.
//!
//! Macros are written `$Name$`. Unknown names stay as they are (so `$HOME` or `$5$` pass
//! through), and a macro whose value is not available (no file open for `$FilePath$`)
//! expands to an empty string and is reported in [`Expansion::missing`]. `$Prompt$` takes
//! the next answer of [`MacroContext::prompts`]: the UI asks once per occurrence.

use serde::{Deserialize, Serialize};

/// What macros expand to: the project, the file and selection in the focused editor (or the
/// file selected in the Project view), the clipboard and the answers to `$Prompt$`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MacroContext {
    /// Absolute project root.
    pub project_dir: Option<String>,
    pub project_name: Option<String>,
    /// Absolute path of the current file.
    pub file_path: Option<String>,
    pub file_encoding: Option<String>,
    pub selected_text: Option<String>,
    /// 1-based caret line and column.
    pub line: Option<u32>,
    pub column: Option<u32>,
    /// 1-based selection bounds.
    pub selection_start_line: Option<u32>,
    pub selection_start_column: Option<u32>,
    pub selection_end_line: Option<u32>,
    pub selection_end_column: Option<u32>,
    pub clipboard: Option<String>,
    /// Answers to the `$Prompt$` dialogs, in order of appearance.
    pub prompts: Vec<String>,
    /// `$PhpExecutable$`: the project's PHP interpreter.
    pub php_executable: Option<String>,
    pub os_name: Option<String>,
    pub user_home: Option<String>,
}

impl MacroContext {
    /// Fill in the project: its directory and name, and make a project-relative
    /// `file_path` (as the UI knows files) absolute.
    pub fn with_project(mut self, root: &str, name: &str) -> Self {
        let sep = if root.contains('\\') && !root.contains('/') {
            '\\'
        } else {
            '/'
        };
        if let Some(f) = &self.file_path {
            let absolute =
                f.starts_with(['/', '\\', '~']) || (f.len() > 2 && f.as_bytes()[1] == b':');
            if !absolute && !root.is_empty() {
                let rel = if sep == '\\' {
                    f.replace('/', "\\")
                } else {
                    f.clone()
                };
                self.file_path = Some(format!("{}{sep}{rel}", root.trim_end_matches(['/', '\\'])));
            }
        }
        if self.project_dir.is_none() && !root.is_empty() {
            self.project_dir = Some(root.to_string());
        }
        if self.project_name.is_none() && !name.is_empty() {
            self.project_name = Some(name.to_string());
        }
        self
    }
}

/// A macro of the "Insert Macro" dialog with its value in the current context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroInfo {
    /// `$FilePath$`.
    pub name: String,
    pub description: String,
    /// The value it would expand to now; `None` when it is not available (or `$Prompt$`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

/// The result of expanding a text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Expansion {
    pub text: String,
    /// Macros without a value here (`$FilePath$` with no file), each once.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
}

type Getter = fn(&MacroContext) -> Option<String>;

/// Name (without `$`), description and value of every macro, sorted by name like
/// PHPStorm's list. `Prompt` is handled by the expander.
const MACROS: &[(&str, &str, Getter)] = &[
    ("ClipboardContent", "Clipboard content", |c| {
        c.clipboard.clone()
    }),
    ("ColumnNumber", "Column number", |c| {
        c.column.map(|n| n.to_string())
    }),
    ("ContentRoot", "Content root path", |c| {
        c.project_dir.clone()
    }),
    ("FileDir", "File directory", |c| {
        c.file_path.as_deref().map(parent).map(str::to_string)
    }),
    ("FileDirName", "File directory name", |c| {
        c.file_path
            .as_deref()
            .map(|p| file_name(parent(p)).to_string())
    }),
    (
        "FileDirRelativeToProjectRoot",
        "File directory relative to the project root",
        |c| relative(c, c.file_path.as_deref().map(parent)?),
    ),
    ("FileEncoding", "File encoding", |c| {
        c.file_path.as_ref()?;
        Some(c.file_encoding.clone().unwrap_or_else(|| "UTF-8".into()))
    }),
    ("FileExt", "File extension", |c| {
        c.file_path
            .as_deref()
            .map(|p| extension(file_name(p)).to_string())
    }),
    ("FileName", "File name", |c| {
        c.file_path.as_deref().map(|p| file_name(p).to_string())
    }),
    (
        "FileNameWithoutAllExtensions",
        "File name without all extensions",
        |c| {
            c.file_path.as_deref().map(|p| {
                let name = file_name(p);
                match name[1.min(name.len())..].find('.') {
                    Some(i) => name[..i + 1].to_string(),
                    None => name.to_string(),
                }
            })
        },
    ),
    (
        "FileNameWithoutExtension",
        "File name without extension",
        |c| {
            c.file_path
                .as_deref()
                .map(|p| stem(file_name(p)).to_string())
        },
    ),
    ("FileParentDir", "File parent directory", |c| {
        c.file_path
            .as_deref()
            .map(|p| parent(parent(p)).to_string())
    }),
    ("FilePath", "File path", |c| c.file_path.clone()),
    (
        "FilePathRelativeToProjectRoot",
        "File path relative to the project root",
        |c| relative(c, c.file_path.as_deref()?),
    ),
    (
        "FileRelativeDir",
        "File directory relative to the project file directory",
        |c| relative(c, c.file_path.as_deref().map(parent)?),
    ),
    (
        "FileRelativePath",
        "File path relative to the project file directory",
        |c| relative(c, c.file_path.as_deref()?),
    ),
    ("LineNumber", "Line number", |c| {
        c.line.map(|n| n.to_string())
    }),
    ("OSName", "Operating system name", |c| c.os_name.clone()),
    ("PhpExecutable", "PHP interpreter of the project", |c| {
        c.php_executable.clone()
    }),
    ("ProjectFileDir", "Project file directory", |c| {
        c.project_dir.clone()
    }),
    ("ProjectName", "Project name", |c| {
        c.project_name
            .clone()
            .or_else(|| c.project_dir.as_deref().map(|d| file_name(d).to_string()))
    }),
    ("Prompt", "Shows a dialog to enter a string", |_| None),
    ("SelectedText", "Text selected in the editor", |c| {
        c.selected_text.clone()
    }),
    (
        "SelectionEndColumn",
        "Selected text end column number",
        |c| c.selection_end_column.map(|n| n.to_string()),
    ),
    ("SelectionEndLine", "Selected text end line number", |c| {
        c.selection_end_line.map(|n| n.to_string())
    }),
    (
        "SelectionStartColumn",
        "Selected text start column number",
        |c| c.selection_start_column.map(|n| n.to_string()),
    ),
    (
        "SelectionStartLine",
        "Selected text start line number",
        |c| c.selection_start_line.map(|n| n.to_string()),
    ),
    ("UserHome", "User home directory", |c| c.user_home.clone()),
];

const PROMPT: &str = "Prompt";

fn trim_end(p: &str) -> &str {
    let t = p.trim_end_matches(['/', '\\']);
    if t.is_empty() && !p.is_empty() {
        &p[..1]
    } else {
        t
    }
}

fn parent(p: &str) -> &str {
    let p = trim_end(p);
    match p.rfind(['/', '\\']) {
        Some(0) => &p[..1],
        Some(i) if i == 2 && p.as_bytes()[1] == b':' => &p[..3],
        Some(i) => &p[..i],
        None => "",
    }
}

fn file_name(p: &str) -> &str {
    let p = trim_end(p);
    p.rsplit(['/', '\\']).next().unwrap_or(p)
}

/// Name without its last extension (`User.test.php` → `User.test`; `.env` stays).
fn stem(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

fn extension(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 => &name[i + 1..],
        _ => "",
    }
}

/// `path` relative to the project directory (`""` for the root itself).
fn relative(c: &MacroContext, path: &str) -> Option<String> {
    let root = trim_end(c.project_dir.as_deref()?).replace('\\', "/");
    let p = path.replace('\\', "/");
    if p == root {
        return Some(String::new());
    }
    p.strip_prefix(&format!("{}/", root.trim_end_matches('/')))
        .map(str::to_string)
}

/// Every macro with its description and its value in `ctx`.
pub fn list(ctx: &MacroContext) -> Vec<MacroInfo> {
    MACROS
        .iter()
        .map(|(name, description, get)| MacroInfo {
            name: format!("${name}$"),
            description: (*description).to_string(),
            preview: get(ctx),
        })
        .collect()
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The `$Name$` macros in `text` that Forge knows, in order (with repeats).
pub fn used(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    scan(text, |name| {
        out.push(name.to_string());
        None
    });
    out
}

/// How many `$Prompt$` dialogs expanding `text` needs.
pub fn prompt_count(text: &str) -> usize {
    used(text).iter().filter(|m| *m == PROMPT).count()
}

/// Walk `text`, calling `replace` for each known macro; `Some` replaces it.
fn scan(text: &str, mut replace: impl FnMut(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('$') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let len = after.find(|c: char| !is_ident(c)).unwrap_or(after.len());
        let name = &after[..len];
        let closed = after[len..].starts_with('$');
        if closed && !name.is_empty() && MACROS.iter().any(|(n, _, _)| *n == name) {
            match replace(name) {
                Some(v) => out.push_str(&v),
                None => {
                    out.push('$');
                    out.push_str(name);
                    out.push('$');
                }
            }
            rest = &after[len + 1..];
        } else {
            out.push('$');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// Expands texts that share one context: `$Prompt$` answers are consumed across calls, so
/// a tool's program, arguments and working directory ask in that order.
pub struct Expander<'a> {
    ctx: &'a MacroContext,
    next_prompt: usize,
    missing: Vec<String>,
}

impl<'a> Expander<'a> {
    pub fn new(ctx: &'a MacroContext) -> Self {
        Self {
            ctx,
            next_prompt: 0,
            missing: Vec::new(),
        }
    }

    pub fn expand(&mut self, text: &str) -> String {
        let ctx = self.ctx;
        let mut missing = Vec::new();
        let mut prompt = self.next_prompt;
        let out = scan(text, |name| {
            let value = if name == PROMPT {
                prompt += 1;
                ctx.prompts.get(prompt - 1).cloned()
            } else {
                MACROS
                    .iter()
                    .find(|(n, _, _)| *n == name)
                    .and_then(|(_, _, get)| get(ctx))
            };
            if value.is_none() {
                missing.push(format!("${name}$"));
            }
            Some(value.unwrap_or_default())
        });
        self.next_prompt = prompt;
        for m in missing {
            if !self.missing.contains(&m) {
                self.missing.push(m);
            }
        }
        out
    }

    /// Macros that had no value so far.
    pub fn missing(&self) -> &[String] {
        &self.missing
    }

    pub fn into_missing(self) -> Vec<String> {
        self.missing
    }
}

/// Expand one text.
pub fn expand(text: &str, ctx: &MacroContext) -> Expansion {
    let mut e = Expander::new(ctx);
    let text = e.expand(text);
    Expansion {
        text,
        missing: e.into_missing(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> MacroContext {
        MacroContext {
            project_dir: Some("/home/u/shop".into()),
            file_path: Some("/home/u/shop/app/Models/User.test.php".into()),
            selected_text: Some("fullName".into()),
            line: Some(12),
            column: Some(5),
            clipboard: Some("copied".into()),
            prompts: vec!["first".into(), "second".into()],
            ..MacroContext::default()
        }
    }

    fn preview(name: &str) -> Option<String> {
        list(&ctx())
            .into_iter()
            .find(|m| m.name == name)
            .unwrap()
            .preview
    }

    #[test]
    fn file_macros_derive_from_the_current_file() {
        assert_eq!(
            preview("$FileDir$").as_deref(),
            Some("/home/u/shop/app/Models")
        );
        assert_eq!(preview("$FileDirName$").as_deref(), Some("Models"));
        assert_eq!(preview("$FileName$").as_deref(), Some("User.test.php"));
        assert_eq!(preview("$FileExt$").as_deref(), Some("php"));
        assert_eq!(
            preview("$FileNameWithoutExtension$").as_deref(),
            Some("User.test")
        );
        assert_eq!(
            preview("$FileNameWithoutAllExtensions$").as_deref(),
            Some("User")
        );
        assert_eq!(
            preview("$FileParentDir$").as_deref(),
            Some("/home/u/shop/app")
        );
        assert_eq!(
            preview("$FilePathRelativeToProjectRoot$").as_deref(),
            Some("app/Models/User.test.php")
        );
        assert_eq!(
            preview("$FileDirRelativeToProjectRoot$").as_deref(),
            Some("app/Models")
        );
        assert_eq!(preview("$ProjectName$").as_deref(), Some("shop"));
        assert_eq!(preview("$Prompt$"), None);
        // Sorted like PHPStorm's list.
        let names: Vec<String> = list(&ctx()).into_iter().map(|m| m.name).collect();
        let mut sorted = names.clone();
        sorted.sort_by_key(|n| n.to_lowercase());
        assert_eq!(names, sorted);
    }

    #[test]
    fn expands_known_macros_and_keeps_everything_else() {
        let e = expand(
            "$FilePath$:$LineNumber$:$ColumnNumber$ $HOME $5$ $Unknown$ $$",
            &ctx(),
        );
        assert_eq!(
            e.text,
            "/home/u/shop/app/Models/User.test.php:12:5 $HOME $5$ $Unknown$ $$"
        );
        assert!(e.missing.is_empty());
        assert_eq!(
            expand("$SelectedText$ $ClipboardContent$", &ctx()).text,
            "fullName copied"
        );
    }

    #[test]
    fn prompts_take_answers_in_order_across_texts() {
        let c = ctx();
        let mut e = Expander::new(&c);
        assert_eq!(e.expand("--a=$Prompt$"), "--a=first");
        assert_eq!(e.expand("$Prompt$ $Prompt$"), "second ");
        assert_eq!(e.missing(), ["$Prompt$"]);
        assert_eq!(prompt_count("$Prompt$ x $Prompt$ $FilePath$"), 2);
        assert_eq!(used("php $FilePath$ $Nope$"), ["FilePath"]);
    }

    #[test]
    fn unavailable_macros_are_empty_and_reported() {
        let e = expand(
            "lint $FilePath$ in $FileDir$ at $LineNumber$",
            &MacroContext::default(),
        );
        assert_eq!(e.text, "lint  in  at ");
        assert_eq!(e.missing, ["$FilePath$", "$FileDir$", "$LineNumber$"]);
    }

    #[test]
    fn project_relative_files_become_absolute() {
        let c = MacroContext {
            file_path: Some("app/User.php".into()),
            ..MacroContext::default()
        }
        .with_project("/home/u/shop/", "shop");
        assert_eq!(c.file_path.as_deref(), Some("/home/u/shop/app/User.php"));
        assert_eq!(c.project_dir.as_deref(), Some("/home/u/shop/"));
        assert_eq!(expand("$FileDirRelativeToProjectRoot$", &c).text, "app");
        let w = MacroContext {
            file_path: Some("app/User.php".into()),
            ..MacroContext::default()
        }
        .with_project(r"C:\work\shop", "shop");
        assert_eq!(w.file_path.as_deref(), Some(r"C:\work\shop\app\User.php"));
        let abs = MacroContext {
            file_path: Some("/etc/hosts".into()),
            project_dir: Some("/x".into()),
            ..MacroContext::default()
        }
        .with_project("/p", "p");
        assert_eq!(abs.file_path.as_deref(), Some("/etc/hosts"));
        assert_eq!(abs.project_dir.as_deref(), Some("/x"));
    }

    #[test]
    fn windows_paths_are_handled() {
        let c = MacroContext {
            project_dir: Some(r"C:\work\shop".into()),
            file_path: Some(r"C:\work\shop\app\User.php".into()),
            ..MacroContext::default()
        };
        assert_eq!(expand("$FileDir$", &c).text, r"C:\work\shop\app");
        assert_eq!(expand("$FileRelativePath$", &c).text, "app/User.php");
        let root_file = MacroContext {
            file_path: Some(r"C:\a.php".into()),
            ..MacroContext::default()
        };
        assert_eq!(expand("$FileDir$", &root_file).text, r"C:\");
        let unix_root = MacroContext {
            file_path: Some("/a.php".into()),
            ..MacroContext::default()
        };
        assert_eq!(expand("$FileDir$", &unix_root).text, "/");
    }
}
