//! The built-in catalog (`catalog.toml`): languages Forge detects in projects and the
//! language servers that serve them.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const CATALOG_TOML: &str = include_str!("../catalog.toml");
const PACK_SERVERS_TOML: &str = include_str!("../pack_servers.toml");

/// A language as detected in a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Language {
    pub id: String,
    pub name: String,
    /// `"ext"` or `"ext:lspLanguageId"`.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Whole file names, same syntax as `extensions`.
    #[serde(default, alias = "file_names")]
    pub file_names: Vec<String>,
}

/// One extension or file name of a language with the LSP language id announced for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePattern {
    /// Extension without the dot, or a whole file name when `whole_name` is set.
    pub pattern: String,
    pub whole_name: bool,
    pub lsp_language_id: String,
}

fn split_entry(entry: &str, default_id: &str) -> (String, String) {
    match entry.split_once(':') {
        Some((p, id)) if !id.is_empty() => (p.to_string(), id.to_string()),
        _ => (entry.to_string(), default_id.to_string()),
    }
}

impl Language {
    pub fn patterns(&self) -> Vec<FilePattern> {
        let ext = self.extensions.iter().map(|e| (e, false));
        let names = self.file_names.iter().map(|n| (n, true));
        ext.chain(names)
            .map(|(entry, whole_name)| {
                let (pattern, lsp_language_id) = split_entry(entry, &self.id);
                FilePattern {
                    pattern,
                    whole_name,
                    lsp_language_id,
                }
            })
            .collect()
    }

    /// Whether `file_name` (no directories) belongs to this language.
    pub fn matches(&self, file_name: &str) -> bool {
        let lower = file_name.to_ascii_lowercase();
        self.patterns().iter().any(|p| {
            if p.whole_name {
                p.pattern.eq_ignore_ascii_case(file_name)
            } else {
                let ext = p.pattern.to_ascii_lowercase();
                lower.len() > ext.len() + 1
                    && lower.ends_with(&ext)
                    && lower.as_bytes()[lower.len() - ext.len() - 1] == b'.'
            }
        })
    }
}

/// Root markers that must exist for a server to start (any of them).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct When {
    /// Root-level file names; a trailing `*` matches a prefix (`eslint.config.*`).
    #[serde(default)]
    pub files: Vec<String>,
    /// `package.json` dependencies (any section).
    #[serde(default)]
    pub packages: Vec<String>,
    /// Root-level files that keep the server from starting (TypeScript in a Deno project
    /// with `deno.json`).
    #[serde(default)]
    pub absent: Vec<String>,
}

impl When {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.packages.is_empty()
    }
}

/// One way to install a server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOption {
    /// `npm`, `pip`, `pipx`, `go`, `cargo`, `rustup`, `gem`, `dotnet`, `brew`, `pacman`,
    /// `apt` or `download`.
    pub tool: String,
    /// Program and arguments, run without a shell. Empty for `download`.
    #[serde(default)]
    pub command: Vec<String>,
    /// Release page for manual downloads.
    #[serde(default)]
    pub url: Option<String>,
    /// `linux`, `macos`, `windows`; empty means every platform.
    #[serde(default)]
    pub platforms: Vec<String>,
}

impl InstallOption {
    /// Tools that need root or a browser: shown, never run by Forge.
    pub fn runnable(&self) -> bool {
        !self.command.is_empty()
            && !matches!(self.tool.as_str(), "apt" | "pacman" | "download")
            && self.command.first().map(String::as_str) != Some("sudo")
    }

    pub fn applies_to(&self, platform: &str) -> bool {
        self.platforms.is_empty() || self.platforms.iter().any(|p| p == platform)
    }

    /// The command as one line for display and copying.
    pub fn display(&self) -> String {
        if self.command.is_empty() {
            return self.url.clone().unwrap_or_default();
        }
        display_args(&self.command)
    }
}

/// A command line for display and copying: arguments with spaces are quoted.
pub fn display_args(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if a.contains(' ') {
                format!("\"{a}\"")
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A language server in the catalog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSpec {
    pub id: String,
    pub name: String,
    pub group: String,
    pub languages: Vec<String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub alternative: Option<String>,
    #[serde(default)]
    pub plugin: Option<String>,
    pub homepage: String,
    pub license: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub when: When,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub install: Vec<InstallOption>,
    /// Built into Forge (php-lsp): nothing to install.
    #[serde(default)]
    pub builtin: bool,
    #[serde(default, alias = "initialization_options")]
    pub initialization_options: Value,
    #[serde(default)]
    pub settings: Value,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
struct CatalogFile {
    language: Vec<Language>,
    server: Vec<ServerSpec>,
}

/// Languages and servers, in catalog (= priority) order.
#[derive(Debug, Clone)]
pub struct Catalog {
    pub languages: Vec<Language>,
    pub servers: Vec<ServerSpec>,
}

impl Catalog {
    pub fn parse(src: &str) -> Result<Self, String> {
        let file: CatalogFile = toml::from_str(src).map_err(|e| e.to_string())?;
        Ok(Catalog {
            languages: file.language,
            servers: file.server,
        })
    }

    /// The built-in catalog.
    pub fn builtin() -> &'static Catalog {
        static CATALOG: OnceLock<Catalog> = OnceLock::new();
        CATALOG.get_or_init(|| Catalog::parse(CATALOG_TOML).expect("built-in catalog.toml"))
    }

    /// Servers that packs start themselves (php-lsp, jdtls, clangd, …): listed, never planned.
    pub fn pack_servers() -> &'static Catalog {
        static PACKS: OnceLock<Catalog> = OnceLock::new();
        PACKS.get_or_init(|| Catalog::parse(PACK_SERVERS_TOML).expect("built-in pack_servers.toml"))
    }

    pub fn language(&self, id: &str) -> Option<&Language> {
        self.languages.iter().find(|l| l.id == id)
    }

    pub fn server(&self, id: &str) -> Option<&ServerSpec> {
        self.servers.iter().find(|s| s.id == id)
    }

    /// Language of a file name, if the catalog knows it.
    pub fn language_of(&self, file_name: &str) -> Option<&Language> {
        // Whole file names first: `docker-compose.yml` is Docker Compose, not YAML.
        self.languages
            .iter()
            .find(|l| {
                l.file_names
                    .iter()
                    .any(|n| split_entry(n, "").0.eq_ignore_ascii_case(file_name))
            })
            .or_else(|| self.languages.iter().find(|l| l.matches(file_name)))
    }

    /// Every file pattern of a server, in language order.
    pub fn patterns_of(&self, server: &ServerSpec) -> Vec<FilePattern> {
        server
            .languages
            .iter()
            .filter_map(|id| self.language(id))
            .flat_map(Language::patterns)
            .collect()
    }

    /// Display names of a server's languages.
    pub fn language_names(&self, server: &ServerSpec) -> Vec<String> {
        server
            .languages
            .iter()
            .filter_map(|id| self.language(id).map(|l| l.name.clone()))
            .collect()
    }

    /// Problems in the catalog (empty when consistent). Checked by a unit test.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut ids = std::collections::HashSet::new();
        for l in &self.languages {
            if !ids.insert(format!("lang:{}", l.id)) {
                out.push(format!("duplicate language {}", l.id));
            }
            if l.extensions.is_empty() && l.file_names.is_empty() {
                out.push(format!("language {} has no extensions or file names", l.id));
            }
        }
        // An extension must belong to one language only, or detection is ambiguous.
        let mut owners: std::collections::HashMap<String, &str> = Default::default();
        for l in &self.languages {
            for p in l.patterns() {
                let key = format!(
                    "{}{}",
                    if p.whole_name { "=" } else { "." },
                    p.pattern.to_ascii_lowercase()
                );
                if let Some(other) = owners.insert(key.clone(), &l.id) {
                    out.push(format!("{key} belongs to {other} and {}", l.id));
                }
            }
        }
        for s in &self.servers {
            if !ids.insert(format!("server:{}", s.id)) {
                out.push(format!("duplicate server {}", s.id));
            }
            if s.command.trim().is_empty() {
                out.push(format!("server {} has no command", s.id));
            }
            if s.languages.is_empty() {
                out.push(format!("server {} serves no language", s.id));
            }
            for l in &s.languages {
                if self.language(l).is_none() {
                    out.push(format!("server {} uses unknown language {l}", s.id));
                }
            }
            if s.install.is_empty() && !s.builtin {
                out.push(format!("server {} has no install hint", s.id));
            }
            for i in &s.install {
                if i.command.is_empty() && i.url.is_none() {
                    out.push(format!(
                        "server {}: {} install has no command or url",
                        s.id, i.tool
                    ));
                }
            }
            if !s.homepage.starts_with("https://") {
                out.push(format!("server {} needs an https homepage", s.id));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_is_consistent() {
        let c = Catalog::builtin();
        assert_eq!(c.problems(), Vec::<String>::new());
        let packs = Catalog::pack_servers();
        assert_eq!(packs.problems(), Vec::<String>::new());
        for s in &packs.servers {
            assert!(c.server(&s.id).is_none(), "{} is listed twice", s.id);
        }
        assert!(packs.server("clangd").is_some(), "CForge starts clangd");
        for id in [
            "typescript-language-server",
            "vtsls",
            "vue-language-server",
            "ngserver",
            "svelteserver",
            "astro-ls",
            "vscode-html-language-server",
            "vscode-css-language-server",
            "vscode-json-language-server",
            "vscode-eslint-language-server",
            "tailwindcss-language-server",
            "yaml-language-server",
            "taplo",
            "lemminx",
            "marksman",
            "docker-langserver",
            "docker-compose-langserver",
            "bash-language-server",
            "sqls",
            "texlab",
            "twiggy-language-server",
            "lua-language-server",
            "basedpyright",
            "pyright",
            "gopls",
            "rust-analyzer",
            "csharp-ls",
            "ruby-lsp",
            "zls",
        ] {
            assert!(c.server(id).is_some(), "missing {id}");
        }
    }

    #[test]
    fn yaml_server_accepts_compose_merge_tags() {
        let yaml = Catalog::builtin().server("yaml-language-server").unwrap();
        let tags: Vec<&str> = yaml.settings["yaml"]["customTags"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t.as_str())
            .collect();
        for tag in ["!reset", "!override"] {
            for kind in ["scalar", "sequence", "mapping"] {
                let entry = format!("{tag} {kind}");
                assert!(tags.contains(&entry.as_str()), "missing {entry}");
            }
        }
    }

    #[test]
    fn problems_are_reported() {
        let c = Catalog::parse(
            r#"
            [[language]]
            id = "a"
            name = "A"
            extensions = ["x"]
            [[language]]
            id = "b"
            name = "B"
            extensions = ["x"]
            [[server]]
            id = "s"
            name = "S"
            group = "G"
            languages = ["a", "nope"]
            command = "s"
            homepage = "http://example.com"
            license = "MIT"
            "#,
        )
        .unwrap();
        let p = c.problems();
        assert!(p.iter().any(|m| m.contains(".x belongs to a and b")));
        assert!(p.iter().any(|m| m.contains("unknown language nope")));
        assert!(p.iter().any(|m| m.contains("no install hint")));
        assert!(p.iter().any(|m| m.contains("https homepage")));
    }

    #[test]
    fn languages_match_extensions_and_file_names() {
        let c = Catalog::builtin();
        let lang = |f: &str| c.language_of(f).map(|l| l.id.as_str());
        assert_eq!(lang("App.tsx"), Some("typescript"));
        assert_eq!(lang("main.RS"), Some("rust"));
        assert_eq!(lang("Dockerfile"), Some("dockerfile"));
        assert_eq!(lang("docker-compose.yml"), Some("dockercompose"));
        assert_eq!(lang("ci.yml"), Some("yaml"));
        assert_eq!(lang("Gemfile"), Some("ruby"));
        assert_eq!(lang("notes.txt"), None);
        let ts = c.language("typescript").unwrap();
        let tsx = ts
            .patterns()
            .into_iter()
            .find(|p| p.pattern == "tsx")
            .unwrap();
        assert_eq!(tsx.lsp_language_id, "typescriptreact");
    }

    #[test]
    fn install_options_know_what_forge_may_run() {
        let c = Catalog::builtin();
        let ts = c.server("typescript-language-server").unwrap();
        let npm = &ts.install[0];
        assert!(npm.runnable());
        assert_eq!(
            npm.display(),
            "npm install -g typescript-language-server typescript"
        );
        let pacman = ts.install.iter().find(|i| i.tool == "pacman").unwrap();
        assert!(!pacman.runnable());
        assert!(pacman.applies_to("linux") && !pacman.applies_to("windows"));
        let lemminx = c.server("lemminx").unwrap();
        assert!(!lemminx.install[0].runnable());
        assert!(lemminx.install[0].display().starts_with("https://"));
    }
}
