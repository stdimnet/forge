//! Project type detection: which languages, frameworks and tools a project uses, read from
//! marker files in the project root and one directory level below it (monorepos with a
//! `backend/` and `frontend/` folder). Versions come from lock files when present, else
//! from the version constraints in the manifests.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A framework (or a language, when used as [`ProjectInfo::primary`]) found in a project.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Framework {
    /// Stable id, e.g. `laravel`, `react`, `love` or a language id like `php`.
    pub id: String,
    /// Display name, e.g. `Laravel` or `LÖVE`.
    pub name: String,
    /// Installed (lock file) or required (manifest constraint) version, e.g. `12.1.3`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Language the framework belongs to (`php`, `javascript`, `lua`, ...).
    pub language: String,
    /// Directory relative to the project root where it was found (`""` for the root).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dir: String,
}

/// What a project is made of. Plugins activate when their project types are present.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    /// Language ids: `php`, `javascript`, `typescript`, `html`, `css`, `lua`, `java`, `rust`, `go`, `python`.
    pub languages: Vec<String>,
    pub frameworks: Vec<Framework>,
    /// Packs whose plugins apply: `phpforge`, `webforge`, `luaforge`, `pyforge`, `javaforge`,
    /// `goforge`, `kotlinforge`.
    pub packs: Vec<String>,
    /// Tooling: `composer`, `npm`, `pnpm`, `yarn`, `bun`, `luarocks`, `uv`, `poetry`, `pipenv`,
    /// `conda`, `maven`, `gradle`, `docker`, `git`.
    pub tools: Vec<String>,
    /// The framework (or, without one, the language) that best describes the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<Framework>,
}

impl ProjectInfo {
    /// Whether the project uses a language, framework, pack or tool with this id.
    pub fn uses(&self, id: &str) -> bool {
        self.languages.iter().any(|l| l == id)
            || self.frameworks.iter().any(|f| f.id == id)
            || self.packs.iter().any(|p| p == id)
            || self.tools.iter().any(|t| t == id)
    }

    /// Activation check for plugins: no project types means "always active".
    pub fn matches_any(&self, project_types: &[String]) -> bool {
        project_types.is_empty() || project_types.iter().any(|t| self.uses(t))
    }

    pub fn framework(&self, id: &str) -> Option<&Framework> {
        self.frameworks.iter().find(|f| f.id == id)
    }
}

/// Display name of a language id.
pub fn language_name(id: &str) -> &str {
    match id {
        "php" => "PHP",
        "javascript" => "JavaScript",
        "typescript" => "TypeScript",
        "html" => "HTML",
        "css" => "CSS",
        "lua" => "Lua",
        "rust" => "Rust",
        "go" => "Go",
        "python" => "Python",
        "java" => "Java",
        "kotlin" => "Kotlin",
        other => other,
    }
}

/// Pack that supports a language, if any.
fn pack_for_language(lang: &str) -> Option<&'static str> {
    match lang {
        "php" => Some("phpforge"),
        "javascript" | "typescript" | "html" | "css" => Some("webforge"),
        "lua" => Some("luaforge"),
        "python" => Some("pyforge"),
        "go" => Some("goforge"),
        "java" => Some("javaforge"),
        "kotlin" => Some("kotlinforge"),
        _ => None,
    }
}

/// Composer packages that identify a framework: (package, id, name).
const COMPOSER_FRAMEWORKS: &[(&str, &str, &str)] = &[
    ("laravel/framework", "laravel", "Laravel"),
    ("symfony/framework-bundle", "symfony", "Symfony"),
    ("doctrine/orm", "doctrine", "Doctrine"),
    ("drupal/core", "drupal", "Drupal"),
    ("drupal/core-recommended", "drupal", "Drupal"),
    ("magento/framework", "magento", "Magento"),
    ("magento/product-community-edition", "magento", "Magento"),
    ("johnpbloch/wordpress", "wordpress", "WordPress"),
    ("roots/wordpress", "wordpress", "WordPress"),
    ("roots/bedrock-autoloader", "wordpress", "WordPress"),
    ("johnpbloch/wordpress-core", "wordpress", "WordPress"),
    ("shopware/core", "shopware", "Shopware"),
    ("shopware/platform", "shopware", "Shopware"),
    ("shopware/storefront", "shopware", "Shopware"),
    ("livewire/livewire", "livewire", "Livewire"),
    ("phpunit/phpunit", "phpunit", "PHPUnit"),
    ("pestphp/pest", "pest", "Pest"),
    ("codeception/codeception", "codeception", "Codeception"),
    ("behat/behat", "behat", "Behat"),
    ("phpspec/phpspec", "phpspec", "PHPSpec"),
];

/// npm packages that identify a framework: (package, id, name).
const NPM_FRAMEWORKS: &[(&str, &str, &str)] = &[
    ("next", "next", "Next.js"),
    ("nuxt", "nuxt", "Nuxt"),
    ("@angular/core", "angular", "Angular"),
    ("react", "react", "React"),
    ("vue", "vue", "Vue.js"),
    ("svelte", "svelte", "Svelte"),
    ("solid-js", "solid", "Solid"),
    ("vite", "vite", "Vite"),
];

/// Most specific first: decides [`ProjectInfo::primary`].
const PRIMARY_PRIORITY: &[&str] = &[
    "spring-boot",
    "jakarta-ee",
    "laravel",
    "symfony",
    "drupal",
    "wordpress",
    "shopware",
    "magento",
    "joomla",
    "next",
    "nuxt",
    "angular",
    "svelte",
    "vue",
    "react",
    "solid",
    "love",
    "neovim",
    "vite",
];

const LANGUAGE_PRIORITY: &[&str] = &[
    "php",
    "kotlin",
    "java",
    "typescript",
    "javascript",
    "lua",
    "rust",
    "go",
    "python",
    "html",
    "css",
];

/// Folders never scanned as sub-projects.
const SKIP_DIRS: &[&str] = &[
    "vendor",
    "node_modules",
    "target",
    "dist",
    "build",
    "storage",
    "var",
    "public",
];

const MAX_SUBDIRS: usize = 64;

#[derive(Default)]
struct Collector {
    languages: Vec<String>,
    frameworks: Vec<Framework>,
    tools: Vec<String>,
    /// PHP version required by the root composer.json, for the language fallback.
    php_version: Option<String>,
}

impl Collector {
    fn language(&mut self, id: &str) {
        if !self.languages.iter().any(|l| l == id) {
            self.languages.push(id.to_string());
        }
    }

    fn tool(&mut self, id: &str) {
        if !self.tools.iter().any(|t| t == id) {
            self.tools.push(id.to_string());
        }
    }

    fn framework(
        &mut self,
        id: &str,
        name: &str,
        language: &str,
        version: Option<String>,
        dir: &str,
    ) {
        self.language(language);
        if let Some(existing) = self.frameworks.iter_mut().find(|f| f.id == id) {
            if existing.version.is_none() && existing.dir == dir {
                existing.version = version;
            }
            return;
        }
        self.frameworks.push(Framework {
            id: id.into(),
            name: name.into(),
            version,
            language: language.into(),
            dir: dir.into(),
        });
    }
}

/// Detect what the project at `root` is made of. Never fails: unreadable or malformed
/// files are skipped.
pub fn detect(root: &Path) -> ProjectInfo {
    let mut c = Collector::default();
    if root.join(".git").exists() {
        c.tool("git");
    }
    scan_dir(root, "", &mut c);
    for (name, path) in subdirs(root) {
        scan_dir(&path, &name, &mut c);
    }
    finish(c)
}

fn subdirs(root: &Path) -> Vec<(String, std::path::PathBuf)> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<_> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            (!name.starts_with('.') && !SKIP_DIRS.contains(&name.as_str()))
                .then(|| (name, e.path()))
        })
        .collect();
    dirs.sort();
    dirs.truncate(MAX_SUBDIRS);
    dirs
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// Files directly in `dir`, by name.
fn file_names(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn scan_dir(dir: &Path, rel: &str, c: &mut Collector) {
    let files = file_names(dir);
    let has = |name: &str| files.iter().any(|f| f == name);
    let has_ext = |ext: &str| files.iter().any(|f| f.ends_with(ext));

    // ---- PHP
    if has("composer.json") {
        scan_composer(dir, rel, c);
    }
    if has("artisan") {
        c.framework("laravel", "Laravel", "php", None, rel);
    }
    if dir.join("bin/console").is_file() && dir.join("config/bundles.php").is_file() {
        c.framework("symfony", "Symfony", "php", None, rel);
    }
    if has("wp-config.php") || has("wp-config-sample.php") || dir.join("wp-includes").is_dir() {
        c.framework("wordpress", "WordPress", "php", wordpress_version(dir), rel);
    } else if dir.join("wp-content").is_dir()
        || wordpress_header(dir, &files)
        || composer_requires_prefix(dir, "roots/")
    {
        c.framework("wordpress", "WordPress", "php", None, rel);
    }
    if dir.join("core/lib/Drupal.php").is_file() || drupal_extension(dir, &files) {
        c.framework("drupal", "Drupal", "php", drupal_version(dir), rel);
    }
    if dir
        .join("administrator/manifests/files/joomla.xml")
        .is_file()
        || joomla_manifest(dir, &files)
    {
        c.framework("joomla", "Joomla", "php", joomla_version(dir), rel);
    }
    if dir.join("engine/Shopware/Kernel.php").is_file()
        || composer_type(dir) == Some("shopware-platform-plugin".to_string())
    {
        c.framework("shopware", "Shopware", "php", None, rel);
    }
    if dir.join("app/etc/env.php").is_file() && dir.join("bin/magento").is_file() {
        c.framework("magento", "Magento", "php", None, rel);
    }
    if has_ext(".php") {
        c.language("php");
    }

    // ---- Web
    if has("package.json") {
        scan_package_json(dir, rel, c, &files);
    }
    if has("angular.json") {
        c.framework("angular", "Angular", "typescript", None, rel);
    }
    if files.iter().any(|f| f.starts_with("vite.config.")) {
        c.framework("vite", "Vite", "javascript", None, rel);
    }
    if has("tsconfig.json") || has_ext(".ts") || has_ext(".tsx") {
        c.language("typescript");
    }
    if has_ext(".js") || has_ext(".mjs") || has_ext(".jsx") {
        c.language("javascript");
    }
    if has_ext(".html") || has_ext(".htm") {
        c.language("html");
    }
    if has_ext(".css") || has_ext(".scss") {
        c.language("css");
    }

    // ---- Lua
    // A lone helper script in a subfolder of another project does not make it a Lua project;
    // Lua files in the root, several of them, or a Lua tool's config file do.
    let lua_files = files.iter().filter(|f| f.ends_with(".lua")).count();
    let lua_config = [
        ".luarc.json",
        ".luarc.jsonc",
        ".luacheckrc",
        "stylua.toml",
        ".stylua.toml",
        "selene.toml",
    ]
    .iter()
    .any(|f| has(f));
    if lua_config || lua_files >= 2 || (rel.is_empty() && lua_files > 0) {
        c.language("lua");
    }
    if has_ext(".rockspec") {
        c.language("lua");
        c.tool("luarocks");
    }
    let main_uses_love = || {
        fs::read_to_string(dir.join("main.lua"))
            .map(|s| s.contains("love."))
            .unwrap_or(false)
    };
    if has("conf.lua") || (has("main.lua") && main_uses_love()) {
        c.framework("love", "LÖVE", "lua", None, rel);
    }
    // Neovim: a config (`init.lua` next to `lua/`, or one that uses `vim.`) or a plugin.
    let neovim_plugin = dir.join("lua").is_dir() && dir.join("plugin").is_dir();
    let neovim_config = has("init.lua")
        && !has("main.lua")
        && (dir.join("lua").is_dir()
            || fs::read_to_string(dir.join("init.lua"))
                .map(|s| s.contains("vim."))
                .unwrap_or(false));
    if neovim_config || neovim_plugin {
        c.framework("neovim", "Neovim", "lua", None, rel);
    }

    // ---- Java (JavaForge): Maven and Gradle builds, Java sources
    scan_java(dir, rel, c, &files);
    scan_kotlin(dir, rel, c, &files);

    // ---- Other languages and tools
    if has("Cargo.toml") {
        c.language("rust");
    }
    // ---- Go
    // A module or workspace file, Go files in the root, or several in a folder.
    let go_files = files.iter().filter(|f| f.ends_with(".go")).count();
    if has("go.mod") || has("go.work") || go_files >= 2 || (rel.is_empty() && go_files > 0) {
        c.language("go");
    }
    if has("go.work") {
        c.tool("go-workspace");
    }
    // ---- Python
    // Like Lua: Python files in the root, several of them in a folder, or a packaging /
    // environment file make it a Python project.
    let py_files = files
        .iter()
        .filter(|f| f.ends_with(".py") || f.ends_with(".pyw"))
        .count();
    let requirements = files.iter().any(|f| {
        (f.starts_with("requirements") || f.starts_with("constraints")) && f.ends_with(".txt")
    });
    if has("pyproject.toml")
        || has("setup.py")
        || has("setup.cfg")
        || has("Pipfile")
        || has("environment.yml")
        || has("environment.yaml")
        || requirements
        || py_files >= 2
        || (rel.is_empty() && py_files > 0)
    {
        c.language("python");
    }
    if has("uv.lock") {
        c.tool("uv");
    }
    if has("poetry.lock") {
        c.tool("poetry");
    }
    if has("Pipfile") || has("Pipfile.lock") {
        c.tool("pipenv");
    }
    if has("environment.yml") || has("environment.yaml") {
        c.tool("conda");
    }
    if has("Dockerfile")
        || has("compose.yaml")
        || has("compose.yml")
        || has("docker-compose.yml")
        || has("docker-compose.yaml")
    {
        c.tool("docker");
    }
}

/// Maven (`pom.xml`, `mvnw`) and Gradle (`build.gradle(.kts)`, `settings.gradle(.kts)`,
/// `gradlew`) builds, and Java sources (`*.java` in the folder or a `src/main/java` tree).
/// A Kotlin-only Gradle build (no Java sources, `kotlin("jvm")`) still counts: JavaForge
/// runs its Gradle tasks. Spring Boot and Jakarta EE are frameworks of the build.
fn scan_java(dir: &Path, rel: &str, c: &mut Collector, files: &[String]) {
    let has = |name: &str| files.iter().any(|f| f == name);
    let maven = has("pom.xml");
    let gradle = [
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
    ]
    .iter()
    .any(|f| has(f));
    let sources = files.iter().any(|f| f.ends_with(".java"))
        || dir.join("src/main/java").is_dir()
        || dir.join("src/test/java").is_dir();
    if maven {
        c.tool("maven");
    }
    if gradle {
        c.tool("gradle");
    }
    if maven || gradle || sources {
        c.language("java");
    }
    // Spring Boot (the Gradle plugin, the Maven parent / BOM or a starter) and Jakarta EE /
    // Java EE APIs (the platform APIs, JAX-RS, CDI, MicroProfile).
    let builds: Vec<String> = ["build.gradle.kts", "build.gradle", "pom.xml"]
        .iter()
        .filter(|f| has(f))
        .filter_map(|f| fs::read_to_string(dir.join(f)).ok())
        .collect();
    if builds
        .iter()
        .any(|b| b.contains("org.springframework.boot") || b.contains("spring-boot-starter"))
    {
        let version = builds.iter().find_map(|b| spring_boot_version(b));
        c.framework("spring-boot", "Spring Boot", "java", version, rel);
    }
    const JAKARTA: &[&str] = &[
        "jakarta.platform",
        "jakarta.jakartaee",
        "javaee-api",
        "javaee-web-api",
        "jakarta.ws.rs",
        "javax.ws.rs",
        "jakarta.enterprise",
        "cdi-api",
        "org.eclipse.microprofile",
    ];
    if let Some(b) = builds
        .iter()
        .find(|b| JAKARTA.iter().any(|m| b.contains(m)))
    {
        c.framework("jakarta-ee", "Jakarta EE", "java", jakarta_version(b), rel);
    }
}

/// The Spring Boot version of a build: the Gradle plugin, the Maven parent or BOM.
fn spring_boot_version(build: &str) -> Option<String> {
    let after = |marker: &str| -> Option<String> {
        let i = build.find(marker)? + marker.len();
        let rest = &build[i..];
        let v: String = rest
            .trim_start_matches(['"', '\'', ')', ' ', ':'])
            .trim_start_matches("version")
            .trim_start_matches([' ', '"', '\'', '('])
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
            .collect();
        (!v.is_empty() && v.chars().next().is_some_and(|c| c.is_ascii_digit())).then_some(v)
    };
    for marker in [
        "id(\"org.springframework.boot\")",
        "id 'org.springframework.boot'",
        "id \"org.springframework.boot\"",
        "spring-boot-gradle-plugin",
        "spring-boot-dependencies",
    ] {
        if let Some(v) = after(marker) {
            return Some(v);
        }
    }
    // Maven: `<artifactId>spring-boot-starter-parent</artifactId>` then `<version>`.
    for artifact in ["spring-boot-starter-parent", "spring-boot-dependencies"] {
        let Some(i) = build.find(&format!("<artifactId>{artifact}</artifactId>")) else {
            continue;
        };
        let v = build[i..]
            .split("<version>")
            .nth(1)
            .and_then(|r| r.split("</version>").next())
            .map(str::trim);
        if let Some(v) = v.filter(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit())) {
            return Some(v.to_string());
        }
    }
    None
}

/// The Jakarta EE / Java EE API version (`jakarta.jakartaee-api:10.0.0`).
fn jakarta_version(build: &str) -> Option<String> {
    for marker in [
        "jakartaee-api:",
        "jakartaee-web-api:",
        "jakartaee-core-api:",
        "javaee-api:",
        "javaee-web-api:",
    ] {
        if let Some(i) = build.find(marker) {
            let v: String = build[i + marker.len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '.')
                .collect();
            if v.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                return Some(v);
            }
        }
    }
    for artifact in [
        "jakarta.jakartaee-api",
        "jakarta.jakartaee-web-api",
        "javaee-api",
        "javaee-web-api",
    ] {
        if let Some(i) = build.find(&format!("<artifactId>{artifact}</artifactId>")) {
            let v = build[i..]
                .split("<version>")
                .nth(1)
                .and_then(|r| r.split("</version>").next())
                .map(str::trim);
            if let Some(v) = v.filter(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit())) {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Kotlin (KotlinForge): Kotlin sources (`*.kt` in the folder, `src/main/kotlin`,
/// `src/test/kotlin`) or a build that applies the Kotlin plugin (`kotlin("jvm")`,
/// `org.jetbrains.kotlin.*`, `kotlin-maven-plugin`). Kotest is a framework of its own.
fn scan_kotlin(dir: &Path, rel: &str, c: &mut Collector, files: &[String]) {
    let sources = files
        .iter()
        .any(|f| f.ends_with(".kt") || (f.ends_with(".kts") && !f.ends_with(".gradle.kts")))
        || dir.join("src/main/kotlin").is_dir()
        || dir.join("src/test/kotlin").is_dir();
    let builds: Vec<String> = ["build.gradle.kts", "build.gradle", "pom.xml"]
        .iter()
        .filter(|f| files.iter().any(|x| x == *f))
        .filter_map(|f| fs::read_to_string(dir.join(f)).ok())
        .collect();
    let plugin = builds.iter().any(|b| {
        b.contains("kotlin(\"jvm\")")
            || b.contains("kotlin(\"multiplatform\")")
            || b.contains("org.jetbrains.kotlin")
            || b.contains("kotlin-maven-plugin")
    });
    if sources || plugin {
        c.language("kotlin");
    }
    if builds.iter().any(|b| b.contains("io.kotest")) {
        c.framework("kotest", "Kotest", "kotlin", None, rel);
    }
}

fn scan_composer(dir: &Path, rel: &str, c: &mut Collector) {
    c.language("php");
    c.tool("composer");
    let Some(manifest) = read_json(&dir.join("composer.json")) else {
        return;
    };
    let mut required: HashMap<String, String> = HashMap::new();
    for section in ["require", "require-dev"] {
        if let Some(obj) = manifest.get(section).and_then(Value::as_object) {
            for (name, constraint) in obj {
                required.insert(name.clone(), constraint.as_str().unwrap_or("").to_string());
            }
        }
    }
    if rel.is_empty() || c.php_version.is_none() {
        if let Some(php) = required.get("php").and_then(|v| version_from_constraint(v)) {
            c.php_version = Some(php);
        }
    }
    let locked = composer_lock_versions(&dir.join("composer.lock"));
    for (package, id, name) in COMPOSER_FRAMEWORKS {
        if let Some(constraint) = required.get(*package) {
            let version = locked
                .get(*package)
                .map(|v| clean_version(v))
                .or_else(|| version_from_constraint(constraint));
            c.framework(id, name, "php", version, rel);
        }
    }
    // Magento and Drupal installs often only require a metapackage; the lock file knows more.
    for (package, id, name) in COMPOSER_FRAMEWORKS {
        if !required.contains_key(*package)
            && matches!(*id, "laravel" | "symfony" | "drupal" | "magento")
        {
            if let Some(v) = locked.get(*package) {
                c.framework(id, name, "php", Some(clean_version(v)), rel);
            }
        }
    }
}

fn composer_lock_versions(path: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(lock) = read_json(path) else {
        return out;
    };
    for section in ["packages", "packages-dev"] {
        for p in lock
            .get(section)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let (Some(name), Some(version)) = (
                p.get("name").and_then(Value::as_str),
                p.get("version").and_then(Value::as_str),
            ) {
                out.insert(name.to_string(), version.to_string());
            }
        }
    }
    out
}

/// The first 8 KiB of a file (plugin and theme headers are at the top).
fn head(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut buf = vec![0; 8192];
    let n = fs::File::open(path).ok()?.read(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf[..n]).into_owned())
}

/// A WordPress plugin (`Plugin Name:` in a PHP file) or theme (`Theme Name:` in style.css).
fn wordpress_header(dir: &Path, files: &[String]) -> bool {
    let has_field = |src: &str, key: &str| {
        src.lines().take(60).any(|l| {
            l.trim_start()
                .trim_start_matches(['*', '#', '/'])
                .trim_start()
                .strip_prefix(key)
                .is_some_and(|r| r.trim_start().starts_with(':'))
        })
    };
    files.iter().any(|f| {
        (f.ends_with(".php") && head(&dir.join(f)).is_some_and(|s| has_field(&s, "Plugin Name")))
            || (f == "style.css" && head(&dir.join(f)).is_some_and(|s| has_field(&s, "Theme Name")))
    })
}

fn composer_requires_prefix(dir: &Path, prefix: &str) -> bool {
    read_json(&dir.join("composer.json")).is_some_and(|m| {
        ["require", "require-dev"].iter().any(|s| {
            m.get(*s)
                .and_then(Value::as_object)
                .is_some_and(|o| o.keys().any(|k| k.starts_with(prefix)))
        })
    })
}

fn composer_type(dir: &Path) -> Option<String> {
    read_json(&dir.join("composer.json"))?
        .get("type")?
        .as_str()
        .map(str::to_string)
}

/// A Drupal module, theme or profile: `<name>.info.yml` with `core_version_requirement`.
fn drupal_extension(dir: &Path, files: &[String]) -> bool {
    files.iter().any(|f| {
        f.ends_with(".info.yml")
            && head(&dir.join(f)).is_some_and(|s| s.contains("core_version_requirement"))
    })
}

fn drupal_version(dir: &Path) -> Option<String> {
    let src = head(&dir.join("core/lib/Drupal.php"))?;
    let at = src.find("const VERSION")?;
    let rest = &src[at..];
    let start = rest.find(['\'', '"'])? + 1;
    let end = start + rest[start..].find(['\'', '"'])?;
    Some(rest[start..end].to_string())
}

/// A Joomla extension: a root XML manifest `<extension type="component" …>`.
fn joomla_manifest(dir: &Path, files: &[String]) -> bool {
    files.iter().any(|f| {
        f.ends_with(".xml")
            && head(&dir.join(f)).is_some_and(|s| {
                s.contains("<extension")
                    && ["component", "module", "plugin", "template", "package"]
                        .iter()
                        .any(|t| s.contains(&format!("type=\"{t}\"")))
            })
    })
}

fn joomla_version(dir: &Path) -> Option<String> {
    let src = head(&dir.join("administrator/manifests/files/joomla.xml"))?;
    let start = src.find("<version>")? + "<version>".len();
    let end = start + src[start..].find("</version>")?;
    Some(src[start..end].trim().to_string())
}

fn wordpress_version(dir: &Path) -> Option<String> {
    let src = fs::read_to_string(dir.join("wp-includes/version.php")).ok()?;
    let line = src
        .lines()
        .find(|l| l.trim_start().starts_with("$wp_version"))?;
    let start = line.find(['\'', '"'])? + 1;
    let end = start + line[start..].find(['\'', '"'])?;
    Some(line[start..end].to_string())
}

fn scan_package_json(dir: &Path, rel: &str, c: &mut Collector, files: &[String]) {
    c.language("javascript");
    let manager = if files.iter().any(|f| f == "pnpm-lock.yaml") {
        "pnpm"
    } else if files.iter().any(|f| f == "yarn.lock") {
        "yarn"
    } else if files.iter().any(|f| f.starts_with("bun.lock")) {
        "bun"
    } else {
        "npm"
    };
    c.tool(manager);
    let Some(manifest) = read_json(&dir.join("package.json")) else {
        return;
    };
    let mut deps: BTreeMap<String, String> = BTreeMap::new();
    for section in ["dependencies", "devDependencies", "peerDependencies"] {
        if let Some(obj) = manifest.get(section).and_then(Value::as_object) {
            for (name, constraint) in obj {
                deps.entry(name.clone())
                    .or_insert_with(|| constraint.as_str().unwrap_or("").to_string());
            }
        }
    }
    if deps.contains_key("typescript") {
        c.language("typescript");
    }
    let lock = read_json(&dir.join("package-lock.json"));
    for (package, id, name) in NPM_FRAMEWORKS {
        let Some(constraint) = deps.get(*package) else {
            continue;
        };
        let version = installed_npm_version(dir, package)
            .or_else(|| {
                lock.as_ref()
                    .and_then(|l| {
                        l.pointer(&format!(
                            "/packages/node_modules~1{}/version",
                            package.replace('/', "~1")
                        ))
                    })
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .or_else(|| version_from_constraint(constraint));
        let language = if *id == "angular" {
            "typescript"
        } else {
            "javascript"
        };
        c.framework(id, name, language, version, rel);
    }
}

fn installed_npm_version(dir: &Path, package: &str) -> Option<String> {
    read_json(&dir.join("node_modules").join(package).join("package.json"))?
        .get("version")?
        .as_str()
        .map(str::to_string)
}

/// `v12.1.3` → `12.1.3`.
fn clean_version(v: &str) -> String {
    v.trim().trim_start_matches('v').to_string()
}

/// The lowest version a constraint allows, roughly: `^12.0` → `12.0`, `~5.4 || ^6.0` → `5.4`,
/// `>=8.2` → `8.2`, `12.x` → `12`. Wildcards and branches give `None`.
pub fn version_from_constraint(constraint: &str) -> Option<String> {
    let first = constraint.split("||").next()?.split('|').next()?;
    let first = first.split(',').next()?.split_whitespace().next()?;
    let v = first.trim_start_matches(['^', '~', '>', '=', '<', 'v', ' ']);
    let v = v.trim_end_matches(".x").trim_end_matches(".*");
    if v.is_empty() || !v.starts_with(|ch: char| ch.is_ascii_digit()) {
        return None;
    }
    let v: String = v
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '.' || *ch == '-')
        .collect();
    Some(v)
}

fn finish(c: Collector) -> ProjectInfo {
    let rank = |id: &str| {
        PRIMARY_PRIORITY
            .iter()
            .position(|p| *p == id)
            .unwrap_or(usize::MAX)
    };
    let mut frameworks = c.frameworks;
    // Root frameworks first, then by specificity; keeps the order stable for the UI.
    frameworks.sort_by_key(|f| (!f.dir.is_empty(), rank(&f.id)));
    let primary = frameworks
        .iter()
        .find(|f| rank(&f.id) != usize::MAX)
        .cloned()
        .or_else(|| {
            LANGUAGE_PRIORITY
                .iter()
                .find(|l| c.languages.iter().any(|x| x == *l))
                .map(|l| Framework {
                    id: (*l).to_string(),
                    name: language_name(l).to_string(),
                    version: if *l == "php" {
                        c.php_version.clone()
                    } else {
                        None
                    },
                    language: (*l).to_string(),
                    dir: String::new(),
                })
        });
    let mut languages = c.languages;
    languages.sort_by_key(|l| {
        LANGUAGE_PRIORITY
            .iter()
            .position(|p| p == l)
            .unwrap_or(usize::MAX)
    });
    let mut packs: Vec<String> = Vec::new();
    for l in &languages {
        if let Some(p) = pack_for_language(l) {
            if !packs.iter().any(|x| x == p) {
                packs.push(p.to_string());
            }
        }
    }
    ProjectInfo {
        languages,
        frameworks,
        packs,
        tools: c.tools,
        primary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn write(root: &Path, path: &str, contents: &str) {
        let p = root.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, contents).unwrap();
    }

    fn ids(info: &ProjectInfo) -> Vec<&str> {
        info.frameworks.iter().map(|f| f.id.as_str()).collect()
    }

    #[test]
    fn laravel_with_lock_file_version_and_vite_frontend() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(
            root,
            "composer.json",
            r#"{"require": {"php": "^8.2", "laravel/framework": "^12.0"}, "require-dev": {"phpunit/phpunit": "^11.5"}}"#,
        );
        write(
            root,
            "composer.lock",
            r#"{"packages": [{"name": "laravel/framework", "version": "v12.4.1"}], "packages-dev": [{"name": "phpunit/phpunit", "version": "11.5.2"}]}"#,
        );
        write(root, "artisan", "#!/usr/bin/env php");
        write(
            root,
            "package.json",
            r#"{"devDependencies": {"vite": "^6.0.11", "laravel-vite-plugin": "^1.2.0"}}"#,
        );
        write(root, "vite.config.js", "");
        fs::create_dir(root.join(".git")).unwrap();

        let info = detect(root);
        assert_eq!(ids(&info), vec!["laravel", "vite", "phpunit"]);
        let laravel = info.framework("laravel").unwrap();
        assert_eq!(laravel.version.as_deref(), Some("12.4.1"));
        assert_eq!(
            info.framework("vite").unwrap().version.as_deref(),
            Some("6.0.11")
        );
        assert_eq!(info.primary.as_ref().unwrap().id, "laravel");
        assert_eq!(info.languages, vec!["php", "javascript"]);
        assert_eq!(info.packs, vec!["phpforge", "webforge"]);
        assert!(info.tools.contains(&"git".to_string()));
        assert!(info.tools.contains(&"composer".to_string()));
        assert!(info.uses("laravel") && info.uses("phpforge") && !info.uses("symfony"));
    }

    #[test]
    fn symfony_doctrine_from_constraints() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "composer.json",
            r#"{"require": {"symfony/framework-bundle": "7.2.*", "doctrine/orm": "^3.3"}}"#,
        );
        write(dir.path(), "bin/console", "");
        write(dir.path(), "config/bundles.php", "<?php return [];");
        let info = detect(dir.path());
        assert_eq!(ids(&info), vec!["symfony", "doctrine"]);
        assert_eq!(
            info.framework("symfony").unwrap().version.as_deref(),
            Some("7.2")
        );
        assert_eq!(
            info.framework("doctrine").unwrap().version.as_deref(),
            Some("3.3")
        );
    }

    #[test]
    fn wordpress_version_from_core_file() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "wp-config.php", "<?php");
        write(
            dir.path(),
            "wp-includes/version.php",
            "<?php\n$wp_version = '6.6.2';\n",
        );
        let info = detect(dir.path());
        let wp = info.primary.unwrap();
        assert_eq!(
            (wp.id.as_str(), wp.version.as_deref()),
            ("wordpress", Some("6.6.2"))
        );
    }

    #[test]
    fn wordpress_plugins_themes_and_content_folders() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "acme-events.php",
            "<?php\n/**\n * Plugin Name: Acme Events\n */\n",
        );
        assert_eq!(detect(dir.path()).primary.unwrap().id, "wordpress");
        let theme = tempfile::tempdir().unwrap();
        write(theme.path(), "style.css", "/*\nTheme Name: Anvil\n*/");
        write(theme.path(), "functions.php", "<?php");
        assert_eq!(detect(theme.path()).primary.unwrap().id, "wordpress");
        let content = tempfile::tempdir().unwrap();
        write(content.path(), "wp-content/plugins/a/a.php", "<?php");
        assert_eq!(detect(content.path()).primary.unwrap().id, "wordpress");
        let bedrock = tempfile::tempdir().unwrap();
        write(
            bedrock.path(),
            "composer.json",
            r#"{"require": {"roots/acorn": "^4"}}"#,
        );
        assert_eq!(detect(bedrock.path()).primary.unwrap().id, "wordpress");
    }

    #[test]
    fn drupal_modules_joomla_and_shopware() {
        let module = tempfile::tempdir().unwrap();
        write(
            module.path(),
            "acme.info.yml",
            "name: Acme\ntype: module\ncore_version_requirement: ^10 || ^11\n",
        );
        assert_eq!(detect(module.path()).primary.unwrap().id, "drupal");
        let joomla = tempfile::tempdir().unwrap();
        write(
            joomla.path(),
            "administrator/manifests/files/joomla.xml",
            "<extension type=\"file\"><version>5.1.2</version></extension>",
        );
        let j = detect(joomla.path()).primary.unwrap();
        assert_eq!(
            (j.id.as_str(), j.version.as_deref()),
            ("joomla", Some("5.1.2"))
        );
        let ext = tempfile::tempdir().unwrap();
        write(
            ext.path(),
            "com_acme.xml",
            "<?xml version=\"1.0\"?>\n<extension type=\"component\"></extension>",
        );
        assert_eq!(detect(ext.path()).primary.unwrap().id, "joomla");
        let sw = tempfile::tempdir().unwrap();
        write(
            sw.path(),
            "composer.json",
            r#"{"require": {"shopware/core": "~6.6.0"}}"#,
        );
        assert_eq!(detect(sw.path()).primary.unwrap().name, "Shopware");
        let sw5 = tempfile::tempdir().unwrap();
        write(sw5.path(), "engine/Shopware/Kernel.php", "<?php");
        assert_eq!(detect(sw5.path()).primary.unwrap().id, "shopware");
    }

    #[test]
    fn drupal_and_magento_from_composer() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "composer.json",
            r#"{"require": {"drupal/core-recommended": "^10.3"}}"#,
        );
        assert_eq!(detect(dir.path()).primary.unwrap().id, "drupal");
        write(
            dir.path(),
            "composer.json",
            r#"{"require": {"magento/product-community-edition": "2.4.7"}}"#,
        );
        assert_eq!(detect(dir.path()).primary.unwrap().name, "Magento");
    }

    #[test]
    fn react_typescript_with_installed_version() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "package.json",
            r#"{"dependencies": {"react": "^19.0.0"}, "devDependencies": {"typescript": "~5.7.2", "vite": "^6.1.0"}}"#,
        );
        write(
            dir.path(),
            "node_modules/react/package.json",
            r#"{"version": "19.1.0"}"#,
        );
        write(dir.path(), "tsconfig.json", "{}");
        write(dir.path(), "pnpm-lock.yaml", "");
        let info = detect(dir.path());
        let primary = info.primary.clone().unwrap();
        assert_eq!(
            (primary.id.as_str(), primary.version.as_deref()),
            ("react", Some("19.1.0"))
        );
        assert_eq!(info.languages, vec!["typescript", "javascript"]);
        assert_eq!(info.tools, vec!["pnpm"]);
        assert_eq!(info.packs, vec!["webforge"]);
    }

    #[test]
    fn version_from_package_lock() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "package.json",
            r#"{"dependencies": {"vue": "^3.5.0"}}"#,
        );
        write(
            dir.path(),
            "package-lock.json",
            r#"{"packages": {"node_modules/vue": {"version": "3.5.13"}}}"#,
        );
        assert_eq!(
            detect(dir.path()).primary.unwrap().version.as_deref(),
            Some("3.5.13")
        );
    }

    #[test]
    fn angular_from_angular_json() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "angular.json", "{}");
        write(
            dir.path(),
            "package.json",
            r#"{"dependencies": {"@angular/core": "^19.2.0"}}"#,
        );
        let primary = detect(dir.path()).primary.unwrap();
        assert_eq!(
            (primary.name.as_str(), primary.version.as_deref()),
            ("Angular", Some("19.2.0"))
        );
    }

    #[test]
    fn love_game_and_neovim_plugin() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "main.lua", "function love.draw() end");
        let info = detect(dir.path());
        assert_eq!(info.primary.unwrap().id, "love");
        assert_eq!(info.packs, vec!["luaforge"]);

        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "lua/telescope-extra/init.lua", "return {}");
        write(dir.path(), "plugin/telescope-extra.lua", "");
        assert_eq!(detect(dir.path()).primary.unwrap().id, "neovim");

        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "init.lua", "vim.opt.number = true");
        assert_eq!(detect(dir.path()).primary.unwrap().id, "neovim");
    }

    #[test]
    fn java_builds_and_sources() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "settings.gradle.kts",
            "rootProject.name = \"inventory\"",
        );
        write(
            dir.path(),
            "app/build.gradle.kts",
            "plugins { application }",
        );
        write(
            dir.path(),
            "app/src/main/java/com/acme/App.java",
            "class App {}",
        );
        let info = detect(dir.path());
        assert_eq!(info.languages, vec!["java"]);
        assert_eq!(info.packs, vec!["javaforge"]);
        assert_eq!(info.tools, vec!["gradle"]);
        assert_eq!(info.primary.as_ref().map(|p| p.name.as_str()), Some("Java"));

        let maven = tempfile::tempdir().unwrap();
        write(maven.path(), "pom.xml", "<project/>");
        assert!(detect(maven.path()).uses("maven"));
        let plain = tempfile::tempdir().unwrap();
        write(plain.path(), "Hello.java", "void main() {}");
        assert!(detect(plain.path()).uses("javaforge"));
    }

    #[test]
    fn spring_boot_and_jakarta_ee() {
        let gradle = tempfile::tempdir().unwrap();
        write(
            gradle.path(),
            "build.gradle.kts",
            "plugins {\n    java\n    id(\"org.springframework.boot\") version \"3.3.2\"\n}\ndependencies { implementation(\"org.springframework.boot:spring-boot-starter-web\") }",
        );
        let info = detect(gradle.path());
        let boot = info.framework("spring-boot").unwrap();
        assert_eq!(
            (boot.name.as_str(), boot.version.as_deref()),
            ("Spring Boot", Some("3.3.2"))
        );
        assert_eq!(
            info.primary.as_ref().map(|p| p.id.as_str()),
            Some("spring-boot")
        );
        assert!(info.uses("javaforge"));

        let maven = tempfile::tempdir().unwrap();
        write(
            maven.path(),
            "pom.xml",
            "<project><parent><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-parent</artifactId>\n<version>3.4.0</version></parent></project>",
        );
        assert_eq!(
            detect(maven.path())
                .framework("spring-boot")
                .unwrap()
                .version
                .as_deref(),
            Some("3.4.0")
        );
        let groovy = tempfile::tempdir().unwrap();
        write(
            groovy.path(),
            "build.gradle",
            "plugins { id 'org.springframework.boot' version '2.7.18' }",
        );
        assert_eq!(
            detect(groovy.path())
                .framework("spring-boot")
                .unwrap()
                .version
                .as_deref(),
            Some("2.7.18")
        );

        let ee = tempfile::tempdir().unwrap();
        write(
            ee.path(),
            "build.gradle",
            "dependencies { compileOnly 'jakarta.platform:jakarta.jakartaee-api:10.0.0' }",
        );
        let info = detect(ee.path());
        assert_eq!(
            info.framework("jakarta-ee").unwrap().version.as_deref(),
            Some("10.0.0")
        );
        assert!(info.framework("spring-boot").is_none());
        let plain = tempfile::tempdir().unwrap();
        write(plain.path(), "pom.xml", "<project/>");
        assert!(detect(plain.path()).frameworks.is_empty());
    }

    #[test]
    fn kotlin_sources_builds_and_kotest() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "settings.gradle.kts",
            "rootProject.name = \"orders\"",
        );
        write(
            dir.path(),
            "build.gradle.kts",
            "plugins { kotlin(\"jvm\") version \"2.2.0\" }\ndependencies { testImplementation(\"io.kotest:kotest-runner-junit5:5.9.1\") }",
        );
        let info = detect(dir.path());
        assert!(info.uses("kotlin") && info.uses("kotlinforge"));
        assert!(
            info.uses("javaforge"),
            "Gradle builds belong to JavaForge too"
        );
        assert!(info.framework("kotest").is_some());
        assert_eq!(
            info.primary.as_ref().map(|p| p.name.as_str()),
            Some("Kotlin")
        );

        let plain = tempfile::tempdir().unwrap();
        write(plain.path(), "hello.main.kts", "println(\"Hi\")");
        assert!(detect(plain.path()).uses("kotlinforge"));
        let gradle_only = tempfile::tempdir().unwrap();
        write(gradle_only.path(), "build.gradle.kts", "plugins { java }");
        assert!(!detect(gradle_only.path()).uses("kotlin"));
    }

    #[test]
    fn lua_markers_and_stray_scripts() {
        // A helper script one level down does not turn a PHP project into a Lua one.
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "composer.json", "{}");
        write(dir.path(), "scripts/build.lua", "print(1)");
        assert!(!detect(dir.path()).languages.contains(&"lua".to_string()));
        // Config files and several Lua files do.
        write(dir.path(), "scripts/deploy.lua", "print(2)");
        assert!(detect(dir.path()).packs.contains(&"luaforge".to_string()));
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "game/stylua.toml", "");
        assert!(detect(dir.path()).languages.contains(&"lua".to_string()));
        // A module folder with `init.lua` is not a Neovim config.
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "init.lua", "return require('mylib.core')");
        write(dir.path(), "app.lua", "");
        let info = detect(dir.path());
        assert!(info.framework("neovim").is_none());
        assert_eq!(info.primary.unwrap().name, "Lua");
        // `init.lua` next to `lua/` is.
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "init.lua", "require('config.options')");
        write(dir.path(), "lua/config/options.lua", "");
        assert_eq!(detect(dir.path()).primary.unwrap().id, "neovim");
    }

    #[test]
    fn plain_lua_with_rockspec() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "tool-1.0-1.rockspec", "");
        write(dir.path(), "tool.lua", "");
        let info = detect(dir.path());
        assert_eq!(info.primary.unwrap().name, "Lua");
        assert!(info.tools.contains(&"luarocks".to_string()));
    }

    #[test]
    fn monorepo_one_level_down() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "backend/composer.json",
            r#"{"require": {"symfony/framework-bundle": "^7.1"}}"#,
        );
        write(
            dir.path(),
            "frontend/package.json",
            r#"{"dependencies": {"vue": "^3.4.0"}}"#,
        );
        write(dir.path(), "compose.yaml", "services: {}");
        write(
            dir.path(),
            "node_modules/react/package.json",
            r#"{"version": "1.0.0"}"#,
        );
        let info = detect(dir.path());
        assert_eq!(ids(&info), vec!["symfony", "vue"]);
        assert_eq!(info.framework("symfony").unwrap().dir, "backend");
        assert_eq!(info.primary.unwrap().id, "symfony");
        assert!(info.tools.contains(&"docker".to_string()));
    }

    #[test]
    fn other_languages_and_empty_dirs() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(detect(dir.path()), ProjectInfo::default());
        write(dir.path(), "Cargo.toml", "[package]");
        write(dir.path(), "go.mod", "module x");
        write(dir.path(), "pyproject.toml", "");
        let info = detect(dir.path());
        assert_eq!(info.languages, vec!["rust", "go", "python"]);
        assert_eq!(info.packs, vec!["goforge", "pyforge"]);
        assert_eq!(info.primary.unwrap().name, "Rust");
    }

    #[test]
    fn go_projects() {
        for marker in ["go.mod", "go.work", "main.go"] {
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), marker, "module x");
            let info = detect(dir.path());
            assert_eq!(info.packs, vec!["goforge"], "{marker}");
            assert_eq!(info.primary.unwrap().id, "go", "{marker}");
        }
        // A module in a sub folder, and go.work workspaces.
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "package.json", "{}");
        write(dir.path(), "tools/gen.go", "");
        assert!(!detect(dir.path()).uses("go"));
        write(dir.path(), "api/go.mod", "module x");
        assert!(detect(dir.path()).uses("goforge"));
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "go.work", "go 1.23");
        assert!(detect(dir.path())
            .tools
            .contains(&"go-workspace".to_string()));
    }

    #[test]
    fn python_projects() {
        // Each marker alone makes a Python project.
        for marker in [
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "requirements-dev.txt",
            "Pipfile",
            "environment.yml",
            "main.py",
        ] {
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), marker, "");
            let info = detect(dir.path());
            assert_eq!(info.packs, vec!["pyforge"], "{marker}");
            assert_eq!(info.primary.unwrap().id, "python", "{marker}");
        }
        // One helper script in a subfolder of another project does not count; two do.
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "composer.json", "{}");
        write(dir.path(), "scripts/gen.py", "");
        assert!(!detect(dir.path()).uses("python"));
        write(dir.path(), "scripts/other.py", "");
        assert!(detect(dir.path()).uses("pyforge"));
        // Tools.
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "pyproject.toml", "");
        write(dir.path(), "uv.lock", "");
        write(dir.path(), "poetry.lock", "");
        let info = detect(dir.path());
        assert!(info.uses("uv") && info.uses("poetry") && !info.uses("pipenv"));
    }

    #[test]
    fn plain_php_reports_required_version() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "composer.json",
            r#"{"require": {"php": ">=8.3"}}"#,
        );
        let primary = detect(dir.path()).primary.unwrap();
        assert_eq!(
            (primary.name.as_str(), primary.version.as_deref()),
            ("PHP", Some("8.3"))
        );
    }

    #[test]
    fn malformed_manifests_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "composer.json", "{not json");
        write(dir.path(), "package.json", "");
        let info = detect(dir.path());
        assert_eq!(info.languages, vec!["php", "javascript"]);
        assert!(info.frameworks.is_empty());
    }

    #[test]
    fn constraints() {
        assert_eq!(version_from_constraint("^12.0").as_deref(), Some("12.0"));
        assert_eq!(
            version_from_constraint("~5.4 || ^6.0").as_deref(),
            Some("5.4")
        );
        assert_eq!(
            version_from_constraint(">=8.2 <9.0").as_deref(),
            Some("8.2")
        );
        assert_eq!(version_from_constraint("12.x").as_deref(), Some("12"));
        assert_eq!(version_from_constraint("v1.2.3").as_deref(), Some("1.2.3"));
        assert_eq!(version_from_constraint("*"), None);
        assert_eq!(version_from_constraint("dev-main"), None);
        assert_eq!(version_from_constraint(""), None);
    }

    #[test]
    fn plugin_activation() {
        let info = ProjectInfo {
            languages: vec!["php".into()],
            frameworks: vec![Framework {
                id: "laravel".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(info.matches_any(&[]));
        assert!(info.matches_any(&["laravel".into()]));
        assert!(!info.matches_any(&["symfony".into(), "lua".into()]));
    }
}
