//! Built-in offline project templates. Files are embedded with `include_str!` and use
//! `{{placeholder}}` variables in both contents and paths (see [`TemplateVars`]).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use forge_core::{ForgeError, Result};

/// When a template file is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    Always,
    /// Only with "Create sample code".
    Sample,
    /// Only without "Create sample code" (a minimal variant of a sample file).
    NoSample,
}

#[derive(Debug, Clone, Copy)]
pub struct TemplateFile {
    /// Target path, may contain placeholders (`lua/{{module}}/init.lua`).
    pub path: &'static str,
    pub contents: &'static str,
    pub when: When,
}

#[derive(Debug, Clone, Copy)]
pub struct Template {
    pub id: &'static str,
    /// Directories that are always created, even when empty.
    pub dirs: &'static [&'static str],
    pub files: &'static [TemplateFile],
}

macro_rules! file {
    ($src:literal => $path:literal) => {
        file!($src => $path, When::Always)
    };
    ($src:literal => $path:literal, $when:expr) => {
        TemplateFile {
            path: $path,
            contents: include_str!(concat!("../templates/", $src)),
            when: $when,
        }
    };
}

const TEMPLATES: &[Template] = &[
    Template {
        id: "empty-php",
        dirs: &["src", "tests"],
        files: &[
            file!("empty-php/composer.json" => "composer.json"),
            file!("empty-php/README.md" => "README.md"),
            file!("shared/phpunit.xml" => "phpunit.xml"),
            file!("shared/php.gitignore" => ".gitignore"),
            file!("shared/Greeter.php" => "src/Greeter.php", When::Sample),
            file!("shared/GreeterTest.php" => "tests/GreeterTest.php", When::Sample),
        ],
    },
    Template {
        id: "composer-package",
        dirs: &["src", "tests"],
        files: &[
            file!("composer-package/composer.json" => "composer.json"),
            file!("composer-package/README.md" => "README.md"),
            file!("composer-package/LICENSE" => "LICENSE"),
            file!("composer-package/gitattributes" => ".gitattributes"),
            file!("shared/phpunit.xml" => "phpunit.xml"),
            file!("shared/php.gitignore" => ".gitignore"),
            file!("shared/Greeter.php" => "src/Greeter.php", When::Sample),
            file!("shared/GreeterTest.php" => "tests/GreeterTest.php", When::Sample),
        ],
    },
    Template {
        id: "wordpress-plugin",
        dirs: &["includes"],
        files: &[
            file!("wordpress-plugin/plugin.php" => "{{package}}.php"),
            file!("wordpress-plugin/class-plugin.php" => "includes/class-{{package}}.php"),
            file!("wordpress-plugin/readme.txt" => "readme.txt"),
            file!("wordpress-plugin/uninstall.php" => "uninstall.php"),
        ],
    },
    Template {
        id: "lua-script",
        dirs: &[],
        files: &[
            file!("lua-script/main.lua" => "main.lua", When::Sample),
            file!("lua-script/main_min.lua" => "main.lua", When::NoSample),
            file!("lua-script/greeter.lua" => "lib/greeter.lua", When::Sample),
            file!("lua-script/luarc.json" => ".luarc.json"),
            file!("lua-script/README.md" => "README.md"),
        ],
    },
    Template {
        id: "love-game",
        dirs: &[],
        files: &[
            file!("love-game/main.lua" => "main.lua", When::Sample),
            file!("love-game/main_min.lua" => "main.lua", When::NoSample),
            file!("love-game/conf.lua" => "conf.lua"),
            file!("love-game/luarc.json" => ".luarc.json"),
            file!("love-game/README.md" => "README.md"),
        ],
    },
    Template {
        id: "neovim-plugin",
        dirs: &["lua/{{module}}", "plugin"],
        files: &[
            file!("neovim-plugin/init.lua" => "lua/{{module}}/init.lua"),
            file!("neovim-plugin/plugin.lua" => "plugin/{{module}}.lua"),
            file!("neovim-plugin/help.txt" => "doc/{{module}}.txt"),
            file!("neovim-plugin/luarc.json" => ".luarc.json"),
            file!("neovim-plugin/README.md" => "README.md"),
        ],
    },
    Template {
        id: "empty-web",
        dirs: &["css", "js"],
        files: &[
            file!("empty-web/index.html" => "index.html"),
            file!("empty-web/style.css" => "css/style.css"),
            file!("empty-web/main.js" => "js/main.js", When::Sample),
            file!("empty-web/README.md" => "README.md"),
        ],
    },
    Template {
        id: "empty",
        dirs: &[],
        files: &[],
    },
];

pub fn template(id: &str) -> Option<&'static Template> {
    TEMPLATES.iter().find(|t| t.id == id)
}

pub fn template_ids() -> impl Iterator<Item = &'static str> {
    TEMPLATES.iter().map(|t| t.id)
}

/// Variables available to templates, derived from the project name and options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateVars(BTreeMap<&'static str, String>);

impl TemplateVars {
    /// `name` is the project folder name, e.g. `my-cool-app`.
    pub fn new(name: &str, options: &BTreeMap<String, serde_json::Value>) -> Self {
        let opt = |key: &str, default: &str| {
            options
                .get(key)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(default)
                .to_string()
        };
        let words = words_of(name);
        let class = identifier(&pascal_case(&words), "App");
        let vendor = kebab_case(&words_of(&opt("vendor", "acme")));
        let vendor = if vendor.is_empty() {
            "acme".into()
        } else {
            vendor
        };
        let package = {
            let k = kebab_case(&words);
            if k.is_empty() {
                "app".to_string()
            } else {
                k
            }
        };
        // Packages get `Vendor\Package`, projects just `Package` (like `App`).
        let namespace = if options.get("vendor").is_some() {
            format!(
                "{}\\{}",
                identifier(&pascal_case(&words_of(&vendor)), "Vendor"),
                class
            )
        } else {
            class.clone()
        };
        let php_version = opt("phpVersion", "8.3");
        let phpunit_version = if php_version.as_str() < "8.2" {
            "^10.5"
        } else {
            "^11.5"
        };
        let lua_runtime = match opt("luaVersion", "5.4").as_str() {
            "5.1" => "Lua 5.1",
            "5.3" => "Lua 5.3",
            "luajit" => "LuaJIT",
            _ => "Lua 5.4",
        };
        let module = identifier(&words.join("_").to_lowercase(), "app");
        let mut vars = BTreeMap::new();
        vars.insert("name", name.to_string());
        vars.insert("title", title_case(&words, name));
        vars.insert("package", package);
        vars.insert("vendor", vendor);
        vars.insert("namespace_json", namespace.replace('\\', "\\\\"));
        vars.insert("namespace", namespace);
        vars.insert("constant", module.to_uppercase());
        vars.insert("module", module);
        vars.insert("class", class);
        vars.insert("php_version", php_version);
        vars.insert("phpunit_version", phpunit_version.to_string());
        vars.insert("lua_runtime", lua_runtime.to_string());
        vars.insert("year", current_year().to_string());
        Self(vars)
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    /// Replace every `{{key}}`; unknown placeholders are left as they are.
    pub fn render(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find("{{") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            match after.find("}}") {
                Some(end) if self.0.contains_key(after[..end].trim()) => {
                    out.push_str(&self.0[after[..end].trim()]);
                    rest = &after[end + 2..];
                }
                _ => {
                    out.push_str("{{");
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        out
    }
}

/// Rendered files of a template: (relative path, contents), plus directories to create.
pub struct RenderedTemplate {
    pub dirs: Vec<PathBuf>,
    pub files: Vec<(PathBuf, String)>,
}

pub fn render_template(id: &str, vars: &TemplateVars, sample: bool) -> Result<RenderedTemplate> {
    let t = template(id).ok_or_else(|| ForgeError::Other(format!("unknown template {id}")))?;
    let dirs = t
        .dirs
        .iter()
        .map(|d| PathBuf::from(vars.render(d)))
        .collect();
    let files = t
        .files
        .iter()
        .filter(|f| match f.when {
            When::Always => true,
            When::Sample => sample,
            When::NoSample => !sample,
        })
        .map(|f| (PathBuf::from(vars.render(f.path)), vars.render(f.contents)))
        .collect();
    Ok(RenderedTemplate { dirs, files })
}

/// Write a rendered template below `target`. Returns the written files (relative).
pub fn write_template(target: &Path, rendered: &RenderedTemplate) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(target).map_err(|e| ForgeError::io(target, e))?;
    for dir in &rendered.dirs {
        let p = target.join(dir);
        std::fs::create_dir_all(&p).map_err(|e| ForgeError::io(&p, e))?;
    }
    let mut written = Vec::new();
    for (rel, contents) in &rendered.files {
        let p = target.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ForgeError::io(parent, e))?;
        }
        std::fs::write(&p, contents).map_err(|e| ForgeError::io(&p, e))?;
        written.push(rel.clone());
    }
    Ok(written)
}

/// Split a name into words: `my-coolApp_2` → `my`, `cool`, `App`, `2`.
fn words_of(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for ch in name.chars() {
        if !ch.is_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
            continue;
        }
        if ch.is_uppercase() && prev_lower && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        prev_lower = ch.is_lowercase() || ch.is_ascii_digit();
        cur.push(ch);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn pascal_case(words: &[String]) -> String {
    words
        .iter()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first
                    .to_uppercase()
                    .chain(chars.flat_map(char::to_lowercase))
                    .collect(),
                None => String::new(),
            }
        })
        .collect()
}

fn kebab_case(words: &[String]) -> String {
    words
        .iter()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join("-")
}

fn title_case(words: &[String], fallback: &str) -> String {
    if words.is_empty() {
        return fallback.to_string();
    }
    pascal_case_words(words).join(" ")
}

fn pascal_case_words(words: &[String]) -> Vec<String> {
    words
        .iter()
        .map(|w| pascal_case(std::slice::from_ref(w)))
        .collect()
}

/// A valid PHP/Lua identifier: ASCII alphanumerics, not starting with a digit.
fn identifier(s: &str, fallback: &str) -> String {
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    match cleaned.chars().next() {
        None => fallback.to_string(),
        Some(c) if c.is_ascii_digit() => format!("{fallback}{cleaned}"),
        Some(_) => cleaned,
    }
}

fn current_year() -> u64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Good enough for a copyright line: 365.2425 days per year since 1970.
    1970 + secs / 31_556_952
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn opts(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn derives_names_from_the_project_name() {
        let vars = TemplateVars::new("my-coolApp_2", &BTreeMap::new());
        assert_eq!(vars.get("class"), Some("MyCoolApp2"));
        assert_eq!(vars.get("namespace"), Some("MyCoolApp2"));
        assert_eq!(vars.get("package"), Some("my-cool-app-2"));
        assert_eq!(vars.get("module"), Some("my_cool_app_2"));
        assert_eq!(vars.get("constant"), Some("MY_COOL_APP_2"));
        assert_eq!(vars.get("title"), Some("My Cool App 2"));

        let vars = TemplateVars::new("2048", &BTreeMap::new());
        assert_eq!(vars.get("class"), Some("App2048"));
        assert_eq!(vars.get("module"), Some("app2048"));
    }

    #[test]
    fn package_namespace_includes_vendor() {
        let vars = TemplateVars::new("http-client", &opts(&[("vendor", json!("Acme Corp"))]));
        assert_eq!(vars.get("vendor"), Some("acme-corp"));
        assert_eq!(vars.get("namespace"), Some("AcmeCorp\\HttpClient"));
        assert_eq!(vars.get("namespace_json"), Some("AcmeCorp\\\\HttpClient"));
    }

    #[test]
    fn render_replaces_known_placeholders_only() {
        let vars = TemplateVars::new("demo", &BTreeMap::new());
        assert_eq!(
            vars.render("{{ title }} {{unknown}} {{"),
            "Demo {{unknown}} {{"
        );
    }

    #[test]
    fn empty_php_composer_json_is_valid_with_psr4_namespace() {
        let vars = TemplateVars::new("shop-api", &opts(&[("phpVersion", json!("8.4"))]));
        let t = render_template("empty-php", &vars, true).unwrap();
        let composer = &t
            .files
            .iter()
            .find(|(p, _)| p == Path::new("composer.json"))
            .unwrap()
            .1;
        let parsed: serde_json::Value = serde_json::from_str(composer).unwrap();
        assert_eq!(parsed["require"]["php"], ">=8.4");
        assert_eq!(parsed["autoload"]["psr-4"]["ShopApi\\"], "src/");
        assert_eq!(
            parsed["autoload-dev"]["psr-4"]["ShopApi\\Tests\\"],
            "tests/"
        );
        let greeter = &t
            .files
            .iter()
            .find(|(p, _)| p == Path::new("src/Greeter.php"))
            .unwrap()
            .1;
        assert!(greeter.contains("namespace ShopApi;"));
        assert!(t.files.iter().any(|(p, _)| p == Path::new(".gitignore")));
    }

    #[test]
    fn sample_files_are_optional() {
        let vars = TemplateVars::new("x", &BTreeMap::new());
        let without = render_template("empty-php", &vars, false).unwrap();
        assert!(!without.files.iter().any(|(p, _)| p.starts_with("src")));
        assert!(without.dirs.contains(&PathBuf::from("src")));

        let love = render_template("love-game", &vars, false).unwrap();
        let mains: Vec<_> = love
            .files
            .iter()
            .filter(|(p, _)| p == Path::new("main.lua"))
            .collect();
        assert_eq!(mains.len(), 1);
        assert!(!mains[0].1.contains("keyboard"));
    }

    #[test]
    fn old_php_gets_phpunit_10() {
        let vars = TemplateVars::new("x", &opts(&[("phpVersion", json!("8.1"))]));
        assert_eq!(vars.get("phpunit_version"), Some("^10.5"));
    }

    #[test]
    fn package_and_plugin_templates_render_paths() {
        let vars = TemplateVars::new("Better Search", &opts(&[("vendor", json!("acme"))]));
        let wp = render_template("wordpress-plugin", &vars, true).unwrap();
        let paths: Vec<_> = wp
            .files
            .iter()
            .map(|(p, _)| p.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(paths.contains(&"better-search.php".to_string()));
        assert!(paths.contains(&"includes/class-better-search.php".to_string()));
        let main = &wp.files[0].1;
        assert!(main.contains("Plugin Name:       Better Search"));
        assert!(main.contains("define('BETTER_SEARCH_VERSION'"));

        let nvim = render_template("neovim-plugin", &vars, true).unwrap();
        let paths: Vec<_> = nvim
            .files
            .iter()
            .map(|(p, _)| p.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(paths.contains(&"lua/better_search/init.lua".to_string()));
        assert!(paths.contains(&"plugin/better_search.lua".to_string()));
        assert!(nvim.files[1].1.contains("BetterSearchGreet"));
    }

    #[test]
    fn lua_runtime_option() {
        let vars = TemplateVars::new("x", &opts(&[("luaVersion", json!("luajit"))]));
        let t = render_template("lua-script", &vars, false).unwrap();
        let luarc = &t
            .files
            .iter()
            .find(|(p, _)| p == Path::new(".luarc.json"))
            .unwrap()
            .1;
        let parsed: serde_json::Value = serde_json::from_str(luarc).unwrap();
        assert_eq!(parsed["runtime.version"], "LuaJIT");
    }

    #[test]
    fn every_template_renders_without_leftover_placeholders() {
        let vars = TemplateVars::new("demo-project", &opts(&[("vendor", json!("acme"))]));
        for id in template_ids() {
            for sample in [true, false] {
                for (path, contents) in render_template(id, &vars, sample).unwrap().files {
                    assert!(
                        !contents.contains("{{"),
                        "{id}: {} has a placeholder",
                        path.display()
                    );
                    assert!(!path.to_string_lossy().contains("{{"));
                }
            }
        }
    }

    #[test]
    fn writes_files_and_empty_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("demo");
        let vars = TemplateVars::new("demo", &BTreeMap::new());
        let rendered = render_template("empty-php", &vars, false).unwrap();
        let written = write_template(&target, &rendered).unwrap();
        assert!(written.contains(&PathBuf::from("composer.json")));
        assert!(target.join("tests").is_dir());
        assert!(target.join("phpunit.xml").is_file());
    }
}
