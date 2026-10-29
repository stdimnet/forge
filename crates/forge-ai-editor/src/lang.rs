//! What the AI features need to know about a language: its id (for the per-language
//! switch), comment syntax, documentation comment style, declaration keywords and test file
//! conventions.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    Php,
    JavaScript,
    TypeScript,
    Lua,
    Python,
    Rust,
    Go,
    Java,
    Kotlin,
    Scala,
    Groovy,
    C,
    Cpp,
    CSharp,
    Ruby,
    Html,
    Css,
    Sql,
    Shell,
    Markdown,
    Json,
    Yaml,
    Other,
}

/// How a language documents a declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocStyle {
    /// `/** ... */` above the declaration (PHPDoc, JSDoc, Javadoc, KDoc, Doxygen).
    Block,
    /// `/// ...` lines above (Rust, C#).
    TripleSlash,
    /// `--- ...` lines above (LuaDoc / EmmyLua).
    TripleDash,
    /// `// Name ...` lines above (Go).
    LineSlash,
    /// `# ...` lines above (Ruby, shell).
    Hash,
    /// A `"""docstring"""` as the first statement of the body (Python).
    Docstring,
}

impl Language {
    /// The id used in settings (`ai.completion.disabledLanguages`) and prompts.
    pub fn id(self) -> &'static str {
        match self {
            Language::Php => "php",
            Language::JavaScript => "javascript",
            Language::TypeScript => "typescript",
            Language::Lua => "lua",
            Language::Python => "python",
            Language::Rust => "rust",
            Language::Go => "go",
            Language::Java => "java",
            Language::Kotlin => "kotlin",
            Language::Scala => "scala",
            Language::Groovy => "groovy",
            Language::C => "c",
            Language::Cpp => "cpp",
            Language::CSharp => "csharp",
            Language::Ruby => "ruby",
            Language::Html => "html",
            Language::Css => "css",
            Language::Sql => "sql",
            Language::Shell => "shell",
            Language::Markdown => "markdown",
            Language::Json => "json",
            Language::Yaml => "yaml",
            Language::Other => "text",
        }
    }

    /// Display name for prompts and messages.
    pub fn name(self) -> &'static str {
        match self {
            Language::Php => "PHP",
            Language::JavaScript => "JavaScript",
            Language::TypeScript => "TypeScript",
            Language::Lua => "Lua",
            Language::Python => "Python",
            Language::Rust => "Rust",
            Language::Go => "Go",
            Language::Java => "Java",
            Language::Kotlin => "Kotlin",
            Language::Scala => "Scala",
            Language::Groovy => "Groovy",
            Language::C => "C",
            Language::Cpp => "C++",
            Language::CSharp => "C#",
            Language::Ruby => "Ruby",
            Language::Html => "HTML",
            Language::Css => "CSS",
            Language::Sql => "SQL",
            Language::Shell => "Shell",
            Language::Markdown => "Markdown",
            Language::Json => "JSON",
            Language::Yaml => "YAML",
            Language::Other => "plain text",
        }
    }

    /// Line comment prefix used for the context header of fill-in-the-middle prompts.
    pub fn line_comment(self) -> Option<&'static str> {
        match self {
            Language::Python | Language::Ruby | Language::Shell | Language::Yaml => Some("#"),
            Language::Lua | Language::Sql => Some("--"),
            Language::Html | Language::Markdown | Language::Json | Language::Css => None,
            Language::Other => None,
            _ => Some("//"),
        }
    }

    pub fn doc_style(self) -> Option<DocStyle> {
        Some(match self {
            Language::Php
            | Language::JavaScript
            | Language::TypeScript
            | Language::Java
            | Language::Kotlin
            | Language::Scala
            | Language::Groovy
            | Language::C
            | Language::Cpp => DocStyle::Block,
            Language::Rust | Language::CSharp => DocStyle::TripleSlash,
            Language::Lua => DocStyle::TripleDash,
            Language::Go => DocStyle::LineSlash,
            Language::Ruby | Language::Shell => DocStyle::Hash,
            Language::Python => DocStyle::Docstring,
            _ => return None,
        })
    }

    /// Name of the documentation format in prompts and menu entries.
    pub fn doc_name(self) -> &'static str {
        match self {
            Language::Php => "PHPDoc",
            Language::JavaScript | Language::TypeScript => "JSDoc",
            Language::Java | Language::Groovy => "Javadoc",
            Language::Kotlin => "KDoc",
            Language::Scala => "Scaladoc",
            Language::C | Language::Cpp => "Doxygen",
            Language::Rust => "rustdoc",
            Language::CSharp => "XML documentation",
            Language::Lua => "LuaDoc (EmmyLua annotations)",
            Language::Go => "Go doc comment",
            Language::Python => "docstring",
            _ => "documentation comment",
        }
    }

    /// The usual test framework, for the "Generate Tests" prompt.
    pub fn test_framework(self) -> &'static str {
        match self {
            Language::Php => "PHPUnit",
            Language::JavaScript | Language::TypeScript => "Vitest (Jest-compatible API)",
            Language::Lua => "busted",
            Language::Python => "pytest",
            Language::Rust => "the built-in test harness (#[cfg(test)] mod tests)",
            Language::Go => "the testing package",
            Language::Java => "JUnit 5",
            Language::Kotlin => "JUnit 5 with kotlin.test",
            Language::Scala => "ScalaTest",
            Language::Groovy => "Spock",
            Language::C | Language::Cpp => "GoogleTest",
            Language::CSharp => "xUnit",
            Language::Ruby => "RSpec",
            _ => "the project's test framework",
        }
    }

    /// Keywords that start a declaration worth documenting (checked per line).
    pub fn is_declaration(self, line: &str) -> bool {
        let t = line.trim_start();
        let words: &[&str] = match self {
            Language::Php => &[
                "function ",
                "public ",
                "protected ",
                "private ",
                "static ",
                "abstract ",
                "final ",
                "class ",
                "interface ",
                "trait ",
                "enum ",
                "readonly ",
            ],
            Language::JavaScript | Language::TypeScript => &[
                "function ",
                "async function",
                "export ",
                "class ",
                "interface ",
                "type ",
                "public ",
                "private ",
                "protected ",
                "static ",
                "async ",
                "const ",
                "abstract ",
            ],
            Language::Lua => &["function ", "local function "],
            Language::Python => &["def ", "async def ", "class "],
            Language::Rust => &[
                "fn ",
                "pub ",
                "async fn ",
                "struct ",
                "enum ",
                "trait ",
                "impl ",
                "const fn ",
                "unsafe fn ",
            ],
            Language::Go => &["func ", "type "],
            Language::Ruby => &["def ", "class ", "module "],
            Language::Shell => &["function "],
            Language::Java
            | Language::Kotlin
            | Language::Scala
            | Language::Groovy
            | Language::CSharp => &[
                "public ",
                "private ",
                "protected ",
                "internal ",
                "static ",
                "fun ",
                "def ",
                "class ",
                "interface ",
                "object ",
                "abstract ",
                "override ",
                "suspend ",
                "data class",
                "enum ",
                "record ",
                "void ",
                "final ",
            ],
            Language::C | Language::Cpp => {
                // `int add(int a, int b) {`: a call-like line that is not a statement.
                return t.contains('(')
                    && !t.ends_with(';')
                    && !t.starts_with("if")
                    && !t.starts_with("for")
                    && !t.starts_with("while")
                    && !t.starts_with("switch")
                    && !t.starts_with("return")
                    && !t.starts_with("//")
                    && !t.starts_with('#');
            }
            _ => &[],
        };
        words.iter().any(|w| t.starts_with(w))
            || (matches!(self, Language::JavaScript | Language::TypeScript) && is_js_method(t))
            || (matches!(self, Language::Shell) && t.contains("()") && t.ends_with('{'))
    }

    /// Lines that import other code (shown to the model when they are outside the prefix).
    pub fn is_import(self, line: &str) -> bool {
        let t = line.trim_start();
        match self {
            Language::Php => {
                t.starts_with("use ") || t.starts_with("namespace ") || t.starts_with("require")
            }
            Language::JavaScript | Language::TypeScript => {
                t.starts_with("import ") || (t.contains("require(") && t.starts_with("const "))
            }
            Language::Lua => t.contains("require"),
            Language::Python => t.starts_with("import ") || t.starts_with("from "),
            Language::Rust => {
                t.starts_with("use ") || t.starts_with("pub use ") || t.starts_with("mod ")
            }
            Language::Go => t.starts_with("import ") || t.starts_with("package "),
            Language::Java | Language::Kotlin | Language::Scala | Language::Groovy => {
                t.starts_with("import ") || t.starts_with("package ")
            }
            Language::C | Language::Cpp => t.starts_with("#include"),
            Language::CSharp => t.starts_with("using ") || t.starts_with("namespace "),
            Language::Ruby => t.starts_with("require"),
            _ => false,
        }
    }
}

/// `name(args) {` or `async name(args) {` (class methods without a keyword).
fn is_js_method(t: &str) -> bool {
    let t = t.strip_prefix("async ").unwrap_or(t);
    let Some(paren) = t.find('(') else {
        return false;
    };
    let name = &t[..paren];
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        && !matches!(
            name,
            "if" | "for" | "while" | "switch" | "catch" | "return" | "function"
        )
        && t.trim_end().ends_with('{')
}

/// The language of a file, by extension (`hint` from the editor wins when it is known).
pub fn language_of(path: &str, hint: Option<&str>) -> Language {
    if let Some(l) = hint.and_then(by_id) {
        return l;
    }
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    if name.ends_with(".blade.php") {
        return Language::Php;
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match ext {
        "php" | "phtml" | "inc" => Language::Php,
        "js" | "mjs" | "cjs" | "jsx" => Language::JavaScript,
        "ts" | "mts" | "cts" | "tsx" | "vue" | "svelte" => Language::TypeScript,
        "lua" => Language::Lua,
        "py" | "pyi" => Language::Python,
        "rs" => Language::Rust,
        "go" => Language::Go,
        "java" => Language::Java,
        "kt" | "kts" => Language::Kotlin,
        "scala" | "sc" => Language::Scala,
        "groovy" | "gradle" => Language::Groovy,
        "c" | "h" => Language::C,
        "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" | "ino" => Language::Cpp,
        "cs" => Language::CSharp,
        "rb" | "rake" => Language::Ruby,
        "html" | "htm" | "twig" => Language::Html,
        "css" | "scss" | "sass" | "less" => Language::Css,
        "sql" => Language::Sql,
        "sh" | "bash" | "zsh" => Language::Shell,
        "md" | "markdown" => Language::Markdown,
        "json" => Language::Json,
        "yml" | "yaml" => Language::Yaml,
        _ => Language::Other,
    }
}

fn by_id(id: &str) -> Option<Language> {
    Some(match id {
        "php" => Language::Php,
        "javascript" | "js" => Language::JavaScript,
        "typescript" | "ts" => Language::TypeScript,
        "lua" => Language::Lua,
        "python" => Language::Python,
        "rust" => Language::Rust,
        "go" => Language::Go,
        "java" => Language::Java,
        "kotlin" => Language::Kotlin,
        "scala" => Language::Scala,
        "groovy" => Language::Groovy,
        "c" => Language::C,
        "cpp" => Language::Cpp,
        "csharp" => Language::CSharp,
        "ruby" => Language::Ruby,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_languages_by_extension_and_hint() {
        assert_eq!(language_of("app/Models/User.php", None), Language::Php);
        assert_eq!(
            language_of("resources/views/a.blade.php", None),
            Language::Php
        );
        assert_eq!(language_of("src/App.tsx", None), Language::TypeScript);
        assert_eq!(language_of("init.lua", None), Language::Lua);
        assert_eq!(language_of("Makefile", None), Language::Other);
        assert_eq!(language_of("x.txt", Some("python")), Language::Python);
        assert_eq!(language_of("x.py", Some("unknown")), Language::Python);
    }

    #[test]
    fn recognizes_declarations_and_imports() {
        assert!(Language::Php.is_declaration("    public function total(): int"));
        assert!(!Language::Php.is_declaration("    return $x;"));
        assert!(Language::TypeScript.is_declaration("  async load(id: string) {"));
        assert!(!Language::TypeScript.is_declaration("  if (x) {"));
        assert!(Language::Python.is_declaration("    def total(self):"));
        assert!(Language::Cpp.is_declaration("int add(int a, int b) {"));
        assert!(!Language::Cpp.is_declaration("  foo(1);"));
        assert!(Language::Php.is_import("use App\\Models\\User;"));
        assert!(Language::Python.is_import("from os import path"));
        assert_eq!(Language::Lua.line_comment(), Some("--"));
        assert_eq!(Language::Python.doc_style(), Some(DocStyle::Docstring));
    }
}
