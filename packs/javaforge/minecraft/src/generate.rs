//! The "Minecraft Plugin" New Project generator, like IntelliJ's Minecraft Development
//! plugin: from the wizard's options it resolves the platform, the Minecraft version, the
//! Java release and the names, then writes the build (Gradle Kotlin DSL, Groovy DSL or
//! Maven) with the platform's repositories and `compileOnly` API, the plugin descriptor
//! (`plugin.yml`, `paper-plugin.yml`, `velocity-plugin.json`, `bungee.yml`), the main class,
//! a `/hello` command, a join listener, `.gitignore`, a README and, with run-paper, the
//! "Run Paper Server" configuration. Nothing here touches the network.

use serde::{Deserialize, Serialize};

use crate::sources::{self, Flavor};
use crate::versions::{self, ApiSpec, Platform};

/// The wizard's options (all strings and booleans, as the New Project dialog sends them).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    /// `paper`, `purpur`, `folia`, `spigot`, `bukkit`, `velocity` or `bungeecord`.
    pub platform: String,
    /// `plugin` (`plugin.yml`) or `paper-plugin` (`paper-plugin.yml`, Paper and forks).
    pub descriptor: String,
    pub mc_version: String,
    /// `gradle-kts`, `gradle-groovy` or `maven`.
    pub build_system: String,
    /// `java` or `kotlin` (Gradle only).
    pub language: String,
    pub group_id: String,
    /// Empty: the main class name.
    pub plugin_name: String,
    /// Fully qualified; empty: `<group id>.<project>.<Project>`.
    pub main_class: String,
    /// Comma-separated.
    pub authors: String,
    pub description: String,
    pub website: String,
    /// Empty: the Minecraft version's (`1.21`).
    pub api_version: String,
    /// run-paper's `runServer` task and the "Run Paper Server" configuration (Gradle).
    pub run_paper: bool,
    /// Shadow (Gradle) / maven-shade-plugin; always on for Kotlin.
    pub shadow: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            platform: "paper".into(),
            descriptor: "plugin".into(),
            mc_version: versions::DEFAULT_MC_VERSION.into(),
            build_system: "gradle-kts".into(),
            language: "java".into(),
            group_id: "com.example".into(),
            plugin_name: String::new(),
            main_class: String::new(),
            authors: String::new(),
            description: String::new(),
            website: String::new(),
            api_version: String::new(),
            run_paper: true,
            shadow: false,
        }
    }
}

impl Options {
    /// Options from the wizard's JSON values; unknown keys (`git`) are ignored.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, String> {
        serde_json::from_value(value.clone()).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Build {
    GradleKts,
    GradleGroovy,
    Maven,
}

impl Build {
    pub fn parse(id: &str) -> Option<Build> {
        match id {
            "gradle-kts" | "" => Some(Build::GradleKts),
            "gradle-groovy" => Some(Build::GradleGroovy),
            "maven" => Some(Build::Maven),
            _ => None,
        }
    }

    pub fn is_gradle(self) -> bool {
        self != Build::Maven
    }
}

/// The plugin descriptor a project uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Descriptor {
    PluginYml,
    PaperPluginYml,
    VelocityJson,
    BungeeYml,
}

impl Descriptor {
    pub fn file_name(self) -> &'static str {
        match self {
            Descriptor::PluginYml => "plugin.yml",
            Descriptor::PaperPluginYml => "paper-plugin.yml",
            Descriptor::VelocityJson => "velocity-plugin.json",
            Descriptor::BungeeYml => "bungee.yml",
        }
    }

    pub fn from_file_name(name: &str) -> Option<Descriptor> {
        [
            Descriptor::PluginYml,
            Descriptor::PaperPluginYml,
            Descriptor::VelocityJson,
            Descriptor::BungeeYml,
        ]
        .into_iter()
        .find(|d| d.file_name() == name)
    }
}

/// Everything the files are generated from, after validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    /// The project folder name (Maven artifact id, Gradle root project).
    pub name: String,
    pub platform: Platform,
    pub build: Build,
    pub kotlin: bool,
    /// Empty for proxies.
    pub mc_version: String,
    pub spec: ApiSpec,
    pub group_id: String,
    /// The main class's package and simple name.
    pub package: String,
    pub class: String,
    pub main_class: String,
    pub plugin_name: String,
    /// Velocity's plugin id (`my-plugin`).
    pub plugin_id: String,
    pub api_version: String,
    pub descriptor: Descriptor,
    pub run_paper: bool,
    pub shadow: bool,
    pub authors: Vec<String>,
    pub description: String,
    pub website: String,
}

pub const VERSION: &str = "1.0.0-SNAPSHOT";

/// A generated file: path relative to the project (`/`-separated) and contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GeneratedFile {
    pub path: String,
    /// Empty for binary files.
    pub contents: String,
    /// The bytes of a binary file (a PNG texture of a mod).
    #[serde(skip)]
    pub binary: Option<Vec<u8>>,
}

const JAVA_KEYWORDS: &[&str] = &[
    "abstract",
    "assert",
    "boolean",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extends",
    "final",
    "finally",
    "float",
    "for",
    "goto",
    "if",
    "implements",
    "import",
    "instanceof",
    "int",
    "interface",
    "long",
    "native",
    "new",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "static",
    "strictfp",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "try",
    "void",
    "volatile",
    "while",
    "true",
    "false",
    "null",
    "var",
    "record",
    "yield",
    "_",
];

/// A Java identifier (ASCII) that is not a keyword.
pub fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
        && !JAVA_KEYWORDS.contains(&s)
}

/// A dotted name of identifiers (`com.example`, `com.example.Main`).
pub fn is_qualified_name(s: &str) -> bool {
    !s.is_empty() && s.split('.').all(is_identifier)
}

/// Split a name into words: `my-coolPlugin_2` → `my`, `cool`, `Plugin`, `2`.
fn words_of(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for ch in name.chars() {
        if !ch.is_ascii_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
            continue;
        }
        if ch.is_ascii_uppercase() && prev_lower && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
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
                    .to_ascii_uppercase()
                    .to_string()
                    .chars()
                    .chain(chars.map(|c| c.to_ascii_lowercase()))
                    .collect(),
                None => String::new(),
            }
        })
        .collect()
}

/// The derived simple class name of a project (`my-plugin` → `MyPlugin`).
pub fn class_name(project: &str) -> String {
    let class = pascal_case(&words_of(project));
    match class.chars().next() {
        None => "Plugin".into(),
        Some(c) if c.is_ascii_digit() => format!("Plugin{class}"),
        Some(_) => class,
    }
}

/// The derived package segment of a project (`my-plugin` → `myplugin`).
pub fn package_segment(project: &str) -> String {
    let seg = words_of(project).join("").to_ascii_lowercase();
    match seg.chars().next() {
        None => "plugin".into(),
        Some(c) if c.is_ascii_digit() => format!("p{seg}"),
        Some(_) if JAVA_KEYWORDS.contains(&seg.as_str()) => format!("{seg}_"),
        Some(_) => seg,
    }
}

/// Velocity's plugin id: lowercase, starts with a letter, `[a-z0-9_-]`, at most 64.
pub fn velocity_id(name: &str) -> String {
    let id = words_of(name).join("-").to_ascii_lowercase();
    let id = if id.starts_with(|c: char| c.is_ascii_lowercase()) {
        id
    } else {
        format!("plugin-{id}").trim_end_matches('-').to_string()
    };
    id.chars().take(64).collect()
}

fn is_plugin_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// Validate the options and derive the names. Options the platform or build does not use
/// are ignored (the wizard hides them).
pub fn resolve(project: &str, o: &Options) -> Result<Project, String> {
    let platform = Platform::parse(o.platform.trim())
        .ok_or_else(|| format!("Unknown platform '{}'", o.platform))?;
    let build = Build::parse(o.build_system.trim())
        .ok_or_else(|| format!("Unknown build system '{}'", o.build_system))?;
    let kotlin = match o.language.trim() {
        "" | "java" => false,
        "kotlin" if build.is_gradle() => true,
        "kotlin" => return Err("Kotlin plugins need a Gradle build".into()),
        other => return Err(format!("Unknown language '{other}'")),
    };
    let mc_version = if platform.is_proxy() {
        String::new()
    } else {
        o.mc_version.trim().to_string()
    };
    let spec = versions::api_spec(platform, &mc_version, build.is_gradle())?;

    let group_id = o.group_id.trim().to_string();
    if !is_qualified_name(&group_id) {
        return Err("Group ID must be a Java package name such as com.example".into());
    }
    let main_class = match o.main_class.trim() {
        "" => format!(
            "{group_id}.{}.{}",
            package_segment(project),
            class_name(project)
        ),
        m => m.to_string(),
    };
    let Some((package, class)) = main_class
        .rsplit_once('.')
        .filter(|_| is_qualified_name(&main_class))
    else {
        return Err(
            "Main class must be a fully qualified class name such as com.example.myplugin.MyPlugin"
                .into(),
        );
    };
    let (package, class) = (package.to_string(), class.to_string());
    if package.starts_with("org.bukkit") || package.starts_with("net.md_5.bungee") {
        return Err(format!(
            "The main class must not be in the server's package {package}"
        ));
    }
    let plugin_name = match o.plugin_name.trim() {
        "" => class.clone(),
        n => n.to_string(),
    };
    if !is_plugin_name(&plugin_name) {
        return Err(
            "Plugin name may only contain letters, digits, '_', '-' and '.' (no spaces)".into(),
        );
    }
    let descriptor = match platform {
        Platform::Velocity => Descriptor::VelocityJson,
        Platform::Bungeecord => Descriptor::BungeeYml,
        p if p.is_paper_family() && o.descriptor == "paper-plugin" => {
            if !versions::supports_paper_plugin_yml(&mc_version) {
                return Err(format!(
                    "paper-plugin.yml needs Minecraft {} or newer; use plugin.yml",
                    versions::PAPER_PLUGIN_YML_MIN
                ));
            }
            Descriptor::PaperPluginYml
        }
        _ => Descriptor::PluginYml,
    };
    let api_version = if platform.is_proxy() {
        String::new()
    } else {
        match o.api_version.trim() {
            "" => versions::api_version(&mc_version),
            v if versions::is_release(v) => v.to_string(),
            v => return Err(format!("'{v}' is not a valid api-version such as 1.21")),
        }
    };
    let authors = o
        .authors
        .split(',')
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_string)
        .collect();
    Ok(Project {
        name: project.trim().to_string(),
        platform,
        build,
        kotlin,
        mc_version,
        spec,
        group_id,
        package,
        class,
        main_class: main_class.clone(),
        plugin_id: velocity_id(&plugin_name),
        plugin_name,
        api_version,
        descriptor,
        run_paper: o.run_paper && build.is_gradle() && !platform.is_proxy(),
        shadow: o.shadow || kotlin,
        authors,
        description: o.description.trim().to_string(),
        website: o.website.trim().to_string(),
    })
}

/// Generate the project's files (sorted by path).
pub fn generate(project: &str, o: &Options) -> Result<Vec<GeneratedFile>, String> {
    Ok(files(&resolve(project, o)?))
}

/// The files of a resolved project (sorted by path).
pub fn files(p: &Project) -> Vec<GeneratedFile> {
    let mut out = Vec::new();
    let mut add = |path: String, contents: String| {
        out.push(GeneratedFile {
            path,
            contents,
            binary: None,
        })
    };
    match p.build {
        Build::GradleKts => {
            add(
                "settings.gradle.kts".into(),
                format!(
                    "plugins {{\n    id(\"org.gradle.toolchains.foojay-resolver-convention\") version \"{}\"\n}}\n\nrootProject.name = \"{}\"\n",
                    versions::FOOJAY_RESOLVER,
                    p.name
                ),
            );
            add("build.gradle.kts".into(), gradle_kts(p));
        }
        Build::GradleGroovy => {
            add(
                "settings.gradle".into(),
                format!(
                    "plugins {{\n    id 'org.gradle.toolchains.foojay-resolver-convention' version '{}'\n}}\n\nrootProject.name = '{}'\n",
                    versions::FOOJAY_RESOLVER,
                    p.name
                ),
            );
            add("build.gradle".into(), gradle_groovy(p));
        }
        Build::Maven => add("pom.xml".into(), pom(p)),
    }
    let (lang_dir, ext) = if p.kotlin {
        ("kotlin", "kt")
    } else {
        ("java", "java")
    };
    let flavor = match (p.platform, p.descriptor) {
        (Platform::Velocity, _) => Flavor::Velocity,
        (Platform::Bungeecord, _) => Flavor::Bungee,
        (_, Descriptor::PaperPluginYml) => Flavor::PaperPlugin,
        (pl, _) if pl.is_paper_family() => Flavor::Paper,
        _ => Flavor::Legacy,
    };
    let src = if p.kotlin {
        sources::kotlin(flavor)
    } else {
        sources::java(flavor)
    };
    let dir = format!("src/main/{lang_dir}/{}", p.package.replace('.', "/"));
    let fill = |t: &str| {
        t.replace("{{package}}", &p.package)
            .replace("{{class}}", &p.class)
            .replace("{{name}}", &p.plugin_name)
    };
    add(format!("{dir}/{}.{ext}", p.class), fill(src.main));
    add(
        format!("{dir}/command/HelloCommand.{ext}"),
        fill(src.command),
    );
    add(
        format!("{dir}/listener/JoinListener.{ext}"),
        fill(src.listener),
    );
    add(
        format!("src/main/resources/{}", p.descriptor.file_name()),
        descriptor(p),
    );
    add(".gitignore".into(), GITIGNORE.into());
    add("README.md".into(), readme(p));
    if p.run_paper {
        add(
            ".forge/runConfigurations.toml".into(),
            RUN_CONFIGURATION.into(),
        );
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// The name of the run configuration generated projects get with run-paper.
pub const RUN_SERVER_CONFIGURATION: &str = "Run Paper Server";

const RUN_CONFIGURATION: &str = r#"selected = "Run Paper Server"

[[configuration]]
name = "Run Paper Server"
type = "gradle"

[configuration.options]
tasks = "runServer"
"#;

const GITIGNORE: &str = "# Gradle and Maven
.gradle/
build/
target/

# The test server of run-paper (runServer)
run/

# IDEs
.idea/
*.iml
out/
.vscode/
.DS_Store
";

// ---- Descriptors ------------------------------------------------------------------------------

/// The version placeholder the build fills in when it processes resources.
fn version_placeholder(p: &Project) -> &'static str {
    if p.build.is_gradle() {
        "${version}"
    } else {
        "${project.version}"
    }
}

/// Literal text in a resource the build filters: Gradle's `expand` treats `$` as a
/// template expression.
fn literal(p: &Project, s: &str) -> String {
    if p.build.is_gradle() {
        s.replace('$', "\\$")
    } else {
        s.to_string()
    }
}

fn yaml_str(p: &Project, s: &str) -> String {
    format!("'{}'", literal(p, &s.replace('\'', "''")))
}

fn json_str(p: &Project, s: &str) -> String {
    literal(p, &serde_json::to_string(s).unwrap_or_default())
}

fn descriptor(p: &Project) -> String {
    let version = version_placeholder(p);
    match p.descriptor {
        Descriptor::VelocityJson => {
            let authors = p
                .authors
                .iter()
                .map(|a| json_str(p, a))
                .collect::<Vec<_>>()
                .join(", ");
            let mut s = String::from("{\n");
            s += &format!("  \"id\": \"{}\",\n", p.plugin_id);
            s += &format!("  \"name\": {},\n", json_str(p, &p.plugin_name));
            s += &format!("  \"version\": \"{version}\",\n");
            if !p.description.is_empty() {
                s += &format!("  \"description\": {},\n", json_str(p, &p.description));
            }
            if !p.website.is_empty() {
                s += &format!("  \"url\": {},\n", json_str(p, &p.website));
            }
            s += &format!("  \"authors\": [{authors}],\n");
            s += "  \"dependencies\": [],\n";
            s += &format!("  \"main\": \"{}\"\n", p.main_class);
            s += "}\n";
            s
        }
        Descriptor::BungeeYml => {
            let mut s = format!(
                "name: {}\nversion: '{version}'\nmain: {}\n",
                p.plugin_name, p.main_class
            );
            if !p.authors.is_empty() {
                s += &format!("author: {}\n", yaml_str(p, &p.authors.join(", ")));
            }
            if !p.description.is_empty() {
                s += &format!("description: {}\n", yaml_str(p, &p.description));
            }
            s
        }
        Descriptor::PluginYml | Descriptor::PaperPluginYml => {
            let mut s = format!(
                "name: {}\nversion: '{version}'\nmain: {}\napi-version: '{}'\n",
                p.plugin_name, p.main_class, p.api_version
            );
            if !p.description.is_empty() {
                s += &format!("description: {}\n", yaml_str(p, &p.description));
            }
            if !p.authors.is_empty() {
                let list = p
                    .authors
                    .iter()
                    .map(|a| yaml_str(p, a))
                    .collect::<Vec<_>>()
                    .join(", ");
                s += &format!("authors: [{list}]\n");
            }
            if !p.website.is_empty() {
                s += &format!("website: {}\n", yaml_str(p, &p.website));
            }
            if p.platform == Platform::Folia {
                s += "folia-supported: true\n";
            }
            if p.descriptor == Descriptor::PluginYml {
                s += "\ncommands:\n  hello:\n    description: Says hello\n    usage: /<command>\n";
            }
            s
        }
    }
}

// ---- Gradle -----------------------------------------------------------------------------------

fn gradle_kts(p: &Project) -> String {
    let java = p.spec.java;
    let mut s = String::from("plugins {\n");
    if p.kotlin {
        s += &format!("    kotlin(\"jvm\") version \"{}\"\n", versions::KOTLIN);
    } else {
        s += "    java\n";
    }
    if p.shadow {
        s += &format!(
            "    id(\"com.gradleup.shadow\") version \"{}\"\n",
            versions::SHADOW
        );
    }
    if p.run_paper {
        s += &format!(
            "    id(\"xyz.jpenilla.run-paper\") version \"{}\"\n",
            versions::RUN_PAPER
        );
    }
    s += "}\n\n";
    s += &format!("group = \"{}\"\nversion = \"{VERSION}\"\n", p.group_id);
    if !p.description.is_empty() {
        s += &format!(
            "description = {}\n",
            serde_json::to_string(&p.description)
                .unwrap_or_default()
                .replace('$', "\\$")
        );
    }
    s += "\nrepositories {\n    mavenCentral()\n";
    for r in &p.spec.repositories {
        s += &format!("    maven(\"{}\")\n", r.url);
    }
    s += "}\n\ndependencies {\n";
    s += &format!("    compileOnly(\"{}\")\n", p.spec.dependency.gradle());
    s += "}\n\n";
    if p.kotlin {
        s += &format!("kotlin {{\n    jvmToolchain({java})\n}}\n\n");
    } else {
        s += &format!(
            "java {{\n    toolchain.languageVersion.set(JavaLanguageVersion.of({java}))\n}}\n\n"
        );
    }
    s += "tasks {\n";
    if !p.kotlin {
        s += &format!(
            "    compileJava {{\n        options.encoding = \"UTF-8\"\n        options.release.set({java})\n    }}\n"
        );
    }
    s += &format!(
        "    processResources {{\n        val props = mapOf(\"version\" to version)\n        inputs.properties(props)\n        filteringCharset = \"UTF-8\"\n        filesMatching(\"{}\") {{\n            expand(props)\n        }}\n    }}\n",
        p.descriptor.file_name()
    );
    if p.shadow {
        s += "    shadowJar {\n        archiveClassifier.set(\"\")\n    }\n    build {\n        dependsOn(shadowJar)\n    }\n";
    }
    if p.run_paper {
        s += &format!(
            "    runServer {{\n        // The Paper version of the test server (./run).\n        minecraftVersion(\"{}\")\n    }}\n",
            p.mc_version
        );
    }
    s += "}\n";
    s
}

fn gradle_groovy(p: &Project) -> String {
    let java = p.spec.java;
    let mut s = String::from("plugins {\n");
    if p.kotlin {
        s += &format!(
            "    id 'org.jetbrains.kotlin.jvm' version '{}'\n",
            versions::KOTLIN
        );
    } else {
        s += "    id 'java'\n";
    }
    if p.shadow {
        s += &format!(
            "    id 'com.gradleup.shadow' version '{}'\n",
            versions::SHADOW
        );
    }
    if p.run_paper {
        s += &format!(
            "    id 'xyz.jpenilla.run-paper' version '{}'\n",
            versions::RUN_PAPER
        );
    }
    s += "}\n\n";
    s += &format!("group = '{}'\nversion = '{VERSION}'\n", p.group_id);
    if !p.description.is_empty() {
        s += &format!(
            "description = '{}'\n",
            p.description.replace('\\', "\\\\").replace('\'', "\\'")
        );
    }
    s += "\nrepositories {\n    mavenCentral()\n";
    for r in &p.spec.repositories {
        s += &format!("    maven {{ url = '{}' }}\n", r.url);
    }
    s += "}\n\ndependencies {\n";
    s += &format!("    compileOnly '{}'\n", p.spec.dependency.gradle());
    s += "}\n\n";
    if p.kotlin {
        s += &format!("kotlin {{\n    jvmToolchain({java})\n}}\n\n");
    } else {
        s += &format!(
            "java {{\n    toolchain.languageVersion = JavaLanguageVersion.of({java})\n}}\n\n"
        );
        s += &format!(
            "tasks.withType(JavaCompile).configureEach {{\n    options.encoding = 'UTF-8'\n    options.release = {java}\n}}\n\n"
        );
    }
    s += &format!(
        "processResources {{\n    def props = [version: version]\n    inputs.properties props\n    filteringCharset 'UTF-8'\n    filesMatching('{}') {{\n        expand props\n    }}\n}}\n",
        p.descriptor.file_name()
    );
    if p.shadow {
        s += "\nshadowJar {\n    archiveClassifier = ''\n}\n\ntasks.named('build') {\n    dependsOn shadowJar\n}\n";
    }
    if p.run_paper {
        s += &format!(
            "\nrunServer {{\n    // The Paper version of the test server (./run).\n    minecraftVersion('{}')\n}}\n",
            p.mc_version
        );
    }
    s
}

// ---- Maven ------------------------------------------------------------------------------------

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn pom(p: &Project) -> String {
    let mut s = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>

"#,
    );
    s += &format!(
        "    <groupId>{}</groupId>\n    <artifactId>{}</artifactId>\n    <version>{VERSION}</version>\n    <packaging>jar</packaging>\n\n",
        p.group_id,
        xml_escape(&artifact_id(&p.name))
    );
    s += &format!("    <name>{}</name>\n", xml_escape(&p.plugin_name));
    if !p.description.is_empty() {
        s += &format!(
            "    <description>{}</description>\n",
            xml_escape(&p.description)
        );
    }
    if !p.website.is_empty() {
        s += &format!("    <url>{}</url>\n", xml_escape(&p.website));
    }
    s += &format!(
        "\n    <properties>\n        <java.version>{}</java.version>\n        <maven.compiler.release>${{java.version}}</maven.compiler.release>\n        <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>\n    </properties>\n\n",
        p.spec.java
    );
    s += "    <repositories>\n";
    for r in &p.spec.repositories {
        s += &format!(
            "        <repository>\n            <id>{}</id>\n            <url>{}</url>\n        </repository>\n",
            r.id, r.url
        );
    }
    s += "    </repositories>\n\n";
    let d = &p.spec.dependency;
    s += &format!(
        "    <dependencies>\n        <dependency>\n            <groupId>{}</groupId>\n            <artifactId>{}</artifactId>\n            <version>{}</version>\n            <scope>provided</scope>\n        </dependency>\n    </dependencies>\n\n",
        d.group, d.artifact, d.version
    );
    s += "    <build>\n        <defaultGoal>clean package</defaultGoal>\n";
    s += "        <resources>\n            <resource>\n                <directory>src/main/resources</directory>\n                <filtering>true</filtering>\n            </resource>\n        </resources>\n";
    s += &format!(
        "        <plugins>\n            <plugin>\n                <groupId>org.apache.maven.plugins</groupId>\n                <artifactId>maven-compiler-plugin</artifactId>\n                <version>{}</version>\n            </plugin>\n",
        versions::MAVEN_COMPILER
    );
    if p.shadow {
        s += &format!(
            "            <plugin>\n                <groupId>org.apache.maven.plugins</groupId>\n                <artifactId>maven-shade-plugin</artifactId>\n                <version>{}</version>\n                <executions>\n                    <execution>\n                        <phase>package</phase>\n                        <goals>\n                            <goal>shade</goal>\n                        </goals>\n                        <configuration>\n                            <createDependencyReducedPom>false</createDependencyReducedPom>\n                        </configuration>\n                    </execution>\n                </executions>\n            </plugin>\n",
            versions::MAVEN_SHADE
        );
    }
    s += "        </plugins>\n    </build>\n</project>\n";
    s
}

/// A Maven artifact id from the project folder name.
fn artifact_id(name: &str) -> String {
    let id = words_of(name).join("-").to_ascii_lowercase();
    if id.is_empty() {
        "plugin".into()
    } else {
        id
    }
}

// ---- README -----------------------------------------------------------------------------------

fn readme(p: &Project) -> String {
    let platform = p.platform.label();
    let mut s = format!("# {}\n\n", p.plugin_name);
    if !p.description.is_empty() {
        s += &format!("{}\n\n", p.description);
    }
    let target = if p.platform.is_proxy() {
        format!("A {platform} proxy plugin")
    } else {
        format!("A {platform} plugin for Minecraft {}", p.mc_version)
    };
    s += &format!(
        "{target}, written in {} (Java {}).\n\n",
        if p.kotlin { "Kotlin" } else { "Java" },
        p.spec.java
    );
    s += "## Build\n\n";
    let jar = if p.build.is_gradle() {
        format!("build/libs/{}-{VERSION}.jar", p.name)
    } else {
        format!("target/{}-{VERSION}.jar", artifact_id(&p.name))
    };
    if p.build.is_gradle() {
        s += "The project has no Gradle wrapper yet. Create it once with an installed Gradle 9 or newer (Shadow 9 does not load on Gradle 8):\n\n```sh\ngradle wrapper\n```\n\nThen build the plugin (a missing JDK of the project's Java release is downloaded by Gradle's toolchain resolver, foojay):\n\n```sh\n./gradlew build\n```\n\n";
    } else {
        s += "```sh\nmvn package\n```\n\n";
    }
    s += &format!("The plugin jar is `{jar}`. Copy it into the `plugins/` folder of your ");
    s += if p.platform.is_proxy() {
        "proxy.\n\n"
    } else {
        "server.\n\n"
    };
    if p.run_paper {
        s += &format!(
            "## Test server\n\n`./gradlew runServer` (or the **{RUN_SERVER_CONFIGURATION}** run configuration in Forge) downloads Paper {} with [run-paper](https://github.com/jpenilla/run-gradle), builds the plugin and starts a server in `run/` with the plugin installed. Accept the Minecraft EULA in `run/eula.txt` on the first start.\n\n",
            p.mc_version
        );
    }
    s += "## Structure\n\n";
    let (dir, ext) = if p.kotlin {
        ("kotlin", "kt")
    } else {
        ("java", "java")
    };
    let pkg = p.package.replace('.', "/");
    s += &format!(
        "- `src/main/{dir}/{pkg}/{}.{ext}`: the main class\n",
        p.class
    );
    s += &format!("- `src/main/{dir}/{pkg}/command/HelloCommand.{ext}`: the `/hello` command\n");
    s += &format!(
        "- `src/main/{dir}/{pkg}/listener/JoinListener.{ext}`: greets players when they join\n"
    );
    s += &format!(
        "- `src/main/resources/{}`: the plugin descriptor\n",
        p.descriptor.file_name()
    );
    if p.platform == Platform::Folia {
        s += "\nThe plugin declares `folia-supported: true`. On Folia, use the region, entity and global schedulers instead of the Bukkit scheduler.\n";
    }
    if p.kotlin {
        s += "\nThe Kotlin standard library is bundled into the plugin jar with Shadow.\n";
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(f: impl FnOnce(&mut Options)) -> Options {
        let mut o = Options::default();
        f(&mut o);
        o
    }

    fn file<'a>(files: &'a [GeneratedFile], path: &str) -> &'a str {
        &files
            .iter()
            .find(|f| f.path == path)
            .unwrap_or_else(|| panic!("{path} missing in {:?}", paths(files)))
            .contents
    }

    fn paths(files: &[GeneratedFile]) -> Vec<&str> {
        files.iter().map(|f| f.path.as_str()).collect()
    }

    #[test]
    fn derives_names() {
        assert_eq!(class_name("my-plugin"), "MyPlugin");
        assert_eq!(class_name("2fa"), "Plugin2fa");
        assert_eq!(package_segment("my-plugin"), "myplugin");
        assert_eq!(package_segment("New"), "new_");
        assert_eq!(velocity_id("MyPlugin"), "my-plugin");
        assert_eq!(velocity_id("2fa"), "plugin-2fa");
        let p = resolve("better-chat", &Options::default()).unwrap();
        assert_eq!(p.main_class, "com.example.betterchat.BetterChat");
        assert_eq!(p.plugin_name, "BetterChat");
        assert_eq!(p.api_version, "1.21");
        assert_eq!(p.spec.java, 21);
    }

    #[test]
    fn validates_options() {
        let err = |f: fn(&mut Options)| resolve("demo", &opts(f)).unwrap_err();
        assert!(err(|o| o.group_id = "com.new".into()).contains("Group ID"));
        assert!(err(|o| o.main_class = "Main".into()).contains("Main class"));
        assert!(err(|o| o.plugin_name = "My Plugin".into()).contains("no spaces"));
        assert!(err(|o| {
            o.language = "kotlin".into();
            o.build_system = "maven".into();
        })
        .contains("Gradle"));
        assert!(err(|o| {
            o.descriptor = "paper-plugin".into();
            o.mc_version = "1.20.4".into();
        })
        .contains("1.20.6"));
        assert!(err(|o| {
            o.platform = "purpur".into();
            o.mc_version = "1.17.1".into();
        })
        .contains("Purpur"));
        assert!(err(|o| o.api_version = "latest".into()).contains("api-version"));
        assert!(err(|o| o.main_class = "org.bukkit.Main".into()).contains("package"));
    }

    #[test]
    fn options_from_wizard_json_ignore_unknown_keys() {
        let o = Options::from_json(&serde_json::json!({
            "platform": "spigot", "git": true, "runPaper": false, "mcVersion": "1.20.4"
        }))
        .unwrap();
        assert_eq!(o.platform, "spigot");
        assert!(!o.run_paper);
        assert_eq!(o.build_system, "gradle-kts");
    }

    #[test]
    fn options_that_do_not_apply_are_ignored() {
        // Proxies: no Minecraft version, run-paper or plugin.yml variant.
        let p = resolve(
            "proxy",
            &opts(|o| {
                o.platform = "velocity".into();
                o.mc_version = "garbage".into();
                o.descriptor = "paper-plugin".into();
            }),
        )
        .unwrap();
        assert!(!p.run_paper);
        assert_eq!(p.descriptor, Descriptor::VelocityJson);
        // Maven has no runServer; Spigot no paper-plugin.yml.
        let p = resolve(
            "x",
            &opts(|o| {
                o.build_system = "maven".into();
                o.platform = "spigot".into();
                o.descriptor = "paper-plugin".into();
            }),
        )
        .unwrap();
        assert!(!p.run_paper);
        assert_eq!(p.descriptor, Descriptor::PluginYml);
        // Kotlin bundles its standard library.
        assert!(
            resolve("x", &opts(|o| o.language = "kotlin".into()))
                .unwrap()
                .shadow
        );
    }

    #[test]
    fn paper_gradle_project_files() {
        let files = generate("my-plugin", &Options::default()).unwrap();
        assert_eq!(
            paths(&files),
            vec![
                ".forge/runConfigurations.toml",
                ".gitignore",
                "README.md",
                "build.gradle.kts",
                "settings.gradle.kts",
                "src/main/java/com/example/myplugin/MyPlugin.java",
                "src/main/java/com/example/myplugin/command/HelloCommand.java",
                "src/main/java/com/example/myplugin/listener/JoinListener.java",
                "src/main/resources/plugin.yml",
            ]
        );
        let build = file(&files, "build.gradle.kts");
        assert!(build.contains("maven(\"https://repo.papermc.io/repository/maven-public/\")"));
        assert!(build.contains("compileOnly(\"io.papermc.paper:paper-api:1.21.11-R0.1-SNAPSHOT\")"));
        assert!(build.contains("id(\"xyz.jpenilla.run-paper\")"));
        assert!(build.contains("minecraftVersion(\"1.21.11\")"));
        assert!(file(&files, "settings.gradle.kts").contains("foojay-resolver-convention"));
        let yml = file(&files, "src/main/resources/plugin.yml");
        assert!(yml.contains("main: com.example.myplugin.MyPlugin\n"));
        assert!(yml.contains("version: '${version}'"));
        assert!(yml.contains("commands:\n  hello:"));
    }

    #[test]
    fn run_configuration_is_a_gradle_run_server_configuration() {
        let files = generate("demo", &Options::default()).unwrap();
        let toml: toml::Value =
            toml::from_str(file(&files, ".forge/runConfigurations.toml")).unwrap();
        let cfg = &toml["configuration"][0];
        assert_eq!(cfg["name"].as_str(), Some(RUN_SERVER_CONFIGURATION));
        assert_eq!(cfg["type"].as_str(), Some("gradle"));
        assert_eq!(cfg["options"]["tasks"].as_str(), Some("runServer"));
        let maven = generate("demo", &opts(|o| o.build_system = "maven".into())).unwrap();
        assert!(!paths(&maven).contains(&".forge/runConfigurations.toml"));
    }

    #[test]
    fn descriptors_escape_text_for_the_build() {
        let files = generate(
            "demo",
            &opts(|o| {
                o.description = "Costs $5, it's 'great'".into();
                o.authors = "Alex, Sam ".into();
                o.website = "https://example.com".into();
                o.platform = "folia".into();
            }),
        )
        .unwrap();
        let yml = file(&files, "src/main/resources/plugin.yml");
        assert!(
            yml.contains("description: 'Costs \\$5, it''s ''great'''"),
            "{yml}"
        );
        assert!(yml.contains("authors: ['Alex', 'Sam']"));
        assert!(yml.contains("folia-supported: true"));
        let velocity = generate(
            "demo",
            &opts(|o| {
                o.platform = "velocity".into();
                o.description = "A \"proxy\" plugin".into();
                o.build_system = "maven".into();
            }),
        )
        .unwrap();
        let json: serde_json::Value =
            serde_json::from_str(file(&velocity, "src/main/resources/velocity-plugin.json"))
                .unwrap();
        assert_eq!(json["id"], "demo");
        assert_eq!(json["main"], "com.example.demo.Demo");
        assert_eq!(json["description"], "A \"proxy\" plugin");
        assert_eq!(json["version"], "${project.version}");
    }

    #[test]
    fn every_combination_generates() {
        for platform in Platform::ALL {
            for build in ["gradle-kts", "gradle-groovy", "maven"] {
                for language in ["java", "kotlin"] {
                    if language == "kotlin" && build == "maven" {
                        continue;
                    }
                    for descriptor in ["plugin", "paper-plugin"] {
                        let o = opts(|o| {
                            o.platform = platform.id().into();
                            o.build_system = build.into();
                            o.language = language.into();
                            o.descriptor = descriptor.into();
                        });
                        let files = generate("demo", &o)
                            .unwrap_or_else(|e| panic!("{platform:?} {build} {language}: {e}"));
                        for f in &files {
                            assert!(!f.contents.contains("{{"), "{}: placeholder", f.path);
                        }
                        let main = files
                            .iter()
                            .find(|f| f.path.contains("/Demo."))
                            .expect("main class");
                        assert!(main.contents.contains("Demo"));
                    }
                }
            }
        }
    }
}
