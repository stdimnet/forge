//! What a Maven or Gradle build looks like, read from its files.

use regex_lite::Regex;
use serde::{Deserialize, Serialize};

/// The build tool of a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BuildTool {
    Maven,
    Gradle,
}

impl BuildTool {
    pub fn id(self) -> &'static str {
        match self {
            BuildTool::Maven => "maven",
            BuildTool::Gradle => "gradle",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BuildTool::Maven => "Maven",
            BuildTool::Gradle => "Gradle",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        match id.trim().to_ascii_lowercase().as_str() {
            "maven" | "mvn" => Some(BuildTool::Maven),
            "gradle" => Some(BuildTool::Gradle),
            _ => None,
        }
    }
}

/// A module (Maven module, Gradle subproject) of a build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Module {
    /// Display name (`app`, `core:util`).
    pub name: String,
    /// Folder relative to the build's root (`app`, `core/util`).
    pub dir: String,
    /// Gradle project path (`:app`, `:core:util`); empty for Maven.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
}

/// A test framework the build depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TestFrameworkUse {
    JUnit5,
    JUnit4,
    TestNg,
}

/// A Maven or Gradle build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildProject {
    pub tool: BuildTool,
    /// `rootProject.name`, `<artifactId>` or the folder's name.
    pub name: String,
    /// The wrapper script exists (`gradlew`, `mvnw`); runs use it by default.
    pub wrapper: bool,
    /// The build file: `build.gradle.kts`, `build.gradle` or `pom.xml`.
    pub build_file: String,
    /// `settings.gradle(.kts)`, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_file: Option<String>,
    /// Gradle Kotlin DSL.
    #[serde(default)]
    pub kotlin_dsl: bool,
    /// Modules besides the root (multi-module builds).
    #[serde(default)]
    pub modules: Vec<Module>,
    /// The JaCoCo plugin is configured (Run with Coverage).
    #[serde(default)]
    pub jacoco: bool,
    /// Gradle's `application` plugin (the `run` task) and its `mainClass`.
    #[serde(default)]
    pub application: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_class: Option<String>,
    /// Spring Boot's plugin (`bootRun`, `spring-boot:run`).
    #[serde(default)]
    pub spring_boot: bool,
    /// Tasks the build registers (`tasks.register("x")`, `task x`).
    #[serde(default)]
    pub custom_tasks: Vec<String>,
    /// Java release of the sources (`JavaLanguageVersion.of(21)`, `maven.compiler.release`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_version: Option<String>,
    #[serde(default)]
    pub test_frameworks: Vec<TestFrameworkUse>,
}

impl BuildProject {
    /// A build known only by its tool and modules (command lines need no more).
    pub fn minimal(tool: BuildTool, modules: Vec<Module>) -> Self {
        BuildProject {
            tool,
            name: String::new(),
            wrapper: false,
            build_file: String::new(),
            settings_file: None,
            kotlin_dsl: false,
            modules,
            jacoco: false,
            application: false,
            main_class: None,
            spring_boot: false,
            custom_tasks: Vec::new(),
            java_version: None,
            test_frameworks: Vec::new(),
        }
    }

    /// The module a project file belongs to: the longest module folder that contains it.
    pub fn module_of(&self, path: &str) -> Option<&Module> {
        let path = path.replace('\\', "/");
        self.modules
            .iter()
            .filter(|m| path.starts_with(&format!("{}/", m.dir)))
            .max_by_key(|m| m.dir.len())
    }

    /// A module by its name, folder or Gradle path.
    pub fn module(&self, key: &str) -> Option<&Module> {
        let key = key.trim().trim_end_matches('/');
        if key.is_empty() {
            return None;
        }
        self.modules
            .iter()
            .find(|m| m.dir == key || m.name == key || m.path == key)
    }
}

const GRADLE_BUILD: [&str; 2] = ["build.gradle.kts", "build.gradle"];
const GRADLE_SETTINGS: [&str; 2] = ["settings.gradle.kts", "settings.gradle"];

/// Read the build in a folder. `files` are the names of the files directly in it, `read`
/// returns a file's text by its path relative to that folder. `prefer` picks the tool when
/// a folder has both a `pom.xml` and a Gradle build (Gradle otherwise).
pub fn detect(
    folder_name: &str,
    files: &[String],
    read: &dyn Fn(&str) -> Option<String>,
    prefer: Option<BuildTool>,
) -> Option<BuildProject> {
    let has = |n: &str| files.iter().any(|f| f == n);
    let gradle_build = GRADLE_BUILD.iter().find(|f| has(f)).copied();
    let gradle_settings = GRADLE_SETTINGS.iter().find(|f| has(f)).copied();
    let is_gradle = gradle_build.is_some() || gradle_settings.is_some();
    let is_maven = has("pom.xml");
    let tool = match (is_gradle, is_maven, prefer) {
        (true, true, Some(t)) => t,
        (true, _, _) => BuildTool::Gradle,
        (false, true, _) => BuildTool::Maven,
        (false, false, _) => return None,
    };
    Some(match tool {
        BuildTool::Gradle => gradle(folder_name, gradle_build, gradle_settings, files, read),
        BuildTool::Maven => maven(folder_name, files, read),
    })
}

fn gradle(
    folder_name: &str,
    build: Option<&str>,
    settings: Option<&str>,
    files: &[String],
    read: &dyn Fn(&str) -> Option<String>,
) -> BuildProject {
    let settings_text = settings.and_then(read).unwrap_or_default();
    let build_file = build
        .unwrap_or(settings.unwrap_or("build.gradle"))
        .to_string();
    let kotlin_dsl = build_file.ends_with(".kts");
    let mut modules = gradle_includes(&settings_text);
    // Subproject build files say more about plugins and dependencies than the root's.
    let mut texts = vec![build.and_then(read).unwrap_or_default()];
    for m in &modules {
        for f in GRADLE_BUILD {
            if let Some(t) = read(&format!("{}/{f}", m.dir)) {
                texts.push(t);
                break;
            }
        }
    }
    let all = texts.join("\n");
    let name = Regex::new(r#"rootProject\.name\s*=\s*["']([^"']+)["']"#)
        .ok()
        .and_then(|re| re.captures(&settings_text).map(|c| c[1].to_string()))
        .unwrap_or_else(|| folder_name.to_string());
    let plugin = |id: &str| {
        let quoted = format!("id(\"{id}\")");
        let groovy = [format!("id '{id}'"), format!("id \"{id}\"")];
        let bare = Regex::new(&format!(r"(?m)^\s*`?{}`?\s*$", regex_lite::escape(id))).ok();
        all.contains(&quoted)
            || groovy.iter().any(|g| all.contains(g.as_str()))
            || all.contains(&format!("apply plugin: '{id}'"))
            || bare.is_some_and(|re| re.is_match(&all))
    };
    let main_class = Regex::new(
        r#"(?:mainClass(?:\.set\(|\s*=\s*|\s+)|mainClassName\s*=\s*)\(?\s*["']([\w.$]+)["']"#,
    )
    .ok()
    .and_then(|re| re.captures(&all).map(|c| c[1].to_string()));
    let application = plugin("application") || main_class.is_some() && all.contains("application");
    let custom_tasks = gradle_tasks(&all);
    modules.sort_by(|a, b| a.path.cmp(&b.path));
    BuildProject {
        tool: BuildTool::Gradle,
        name,
        wrapper: files.iter().any(|f| f == "gradlew" || f == "gradlew.bat"),
        build_file,
        settings_file: settings.map(str::to_string),
        kotlin_dsl,
        modules,
        jacoco: plugin("jacoco"),
        application,
        main_class,
        spring_boot: all.contains("org.springframework.boot"),
        custom_tasks,
        java_version: java_version_gradle(&all),
        test_frameworks: test_frameworks(&all, true),
    }
}

/// Subprojects named by `include` in a settings file (Groovy or Kotlin).
fn gradle_includes(settings: &str) -> Vec<Module> {
    let Ok(quoted) = Regex::new(r#"["']([^"']+)["']"#) else {
        return Vec::new();
    };
    let mut out: Vec<Module> = Vec::new();
    let mut in_include = false;
    for line in settings.lines() {
        let line = line.split("//").next().unwrap_or_default().trim();
        let starts = line.starts_with("include") && !line.starts_with("includeBuild");
        if !starts && !in_include {
            continue;
        }
        for c in quoted.captures_iter(line) {
            let path = c[1].trim().to_string();
            let trimmed = path.trim_start_matches(':');
            if trimmed.is_empty() {
                continue;
            }
            let module = Module {
                name: trimmed.to_string(),
                dir: trimmed.replace(':', "/"),
                path: format!(":{trimmed}"),
            };
            if !out.iter().any(|m| m.path == module.path) {
                out.push(module);
            }
        }
        // `include(` spanning lines (Kotlin DSL lists).
        let opens = line.matches('(').count();
        let closes = line.matches(')').count();
        if starts {
            in_include = opens > closes || line.ends_with(',');
        } else if closes > opens || !line.ends_with(',') {
            in_include = false;
        }
    }
    out
}

fn gradle_tasks(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let patterns = [
        r#"tasks\.register(?:<[^>]*>)?\(\s*["']([\w-]+)["']"#,
        r#"(?m)^\s*task\s+([\w-]+)"#,
        r#"tasks\.create\(\s*["']([\w-]+)["']"#,
    ];
    for p in patterns {
        if let Ok(re) = Regex::new(p) {
            for c in re.captures_iter(text) {
                let t = c[1].to_string();
                if !out.contains(&t) {
                    out.push(t);
                }
            }
        }
    }
    out
}

fn java_version_gradle(text: &str) -> Option<String> {
    let patterns = [
        r"JavaLanguageVersion\.of\(\s*(\d+)\s*\)",
        r"JavaVersion\.VERSION_(?:1_)?(\d+)",
        r#"(?:sourceCompatibility|targetCompatibility)\s*=\s*['"]?(?:1\.)?(\d+)"#,
        r"jvmToolchain\(\s*(\d+)\s*\)",
    ];
    patterns
        .iter()
        .find_map(|p| Regex::new(p).ok()?.captures(text).map(|c| c[1].to_string()))
}

fn test_frameworks(text: &str, gradle: bool) -> Vec<TestFrameworkUse> {
    let mut out = Vec::new();
    let jupiter = text.contains("junit-jupiter")
        || text.contains("org.junit.jupiter")
        || (gradle && text.contains("useJUnitPlatform"))
        || text.contains("junit-bom");
    if jupiter {
        out.push(TestFrameworkUse::JUnit5);
    }
    let junit4 = text.contains("junit:junit")
        || Regex::new(r"<groupId>\s*junit\s*</groupId>")
            .ok()
            .is_some_and(|re| re.is_match(text))
        || text.contains("junit-vintage");
    if junit4 {
        out.push(TestFrameworkUse::JUnit4);
    }
    if text.contains("org.testng") || text.contains("useTestNG") {
        out.push(TestFrameworkUse::TestNg);
    }
    out
}

fn tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&format!("</{name}>"))? + start;
    Some(xml[start..end].trim())
}

/// The pom without `<parent>`, `<dependencies>` and `<build>`, for the project's own
/// `<artifactId>`.
fn pom_head(pom: &str) -> String {
    let mut out = pom.to_string();
    for section in [
        "parent",
        "dependencies",
        "dependencyManagement",
        "build",
        "profiles",
    ] {
        let open = format!("<{section}>");
        let close = format!("</{section}>");
        while let (Some(s), Some(e)) = (out.find(&open), out.find(&close)) {
            if e < s {
                break;
            }
            out.replace_range(s..e + close.len(), "");
        }
    }
    out
}

fn strip_xml_comments(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(s) = rest.find("<!--") {
        out.push_str(&rest[..s]);
        match rest[s..].find("-->") {
            Some(e) => rest = &rest[s + e + 3..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

fn maven(
    folder_name: &str,
    files: &[String],
    read: &dyn Fn(&str) -> Option<String>,
) -> BuildProject {
    let pom = strip_xml_comments(&read("pom.xml").unwrap_or_default());
    let mut modules = Vec::new();
    if let Ok(re) = Regex::new(r"<module>\s*([^<]+?)\s*</module>") {
        for c in re.captures_iter(&pom) {
            let dir = c[1].trim().trim_end_matches('/').to_string();
            modules.push(Module {
                name: dir.rsplit('/').next().unwrap_or(&dir).to_string(),
                dir,
                path: String::new(),
            });
        }
    }
    let mut texts = vec![pom.clone()];
    for m in &modules {
        if let Some(t) = read(&format!("{}/pom.xml", m.dir)) {
            texts.push(strip_xml_comments(&t));
        }
    }
    let all = texts.join("\n");
    let head = pom_head(&pom);
    let name = tag(&head, "artifactId")
        .map(str::to_string)
        .unwrap_or_else(|| folder_name.to_string());
    let java_version = [
        "maven.compiler.release",
        "release",
        "maven.compiler.source",
        "java.version",
    ]
    .iter()
    .find_map(|t| tag(&all, t))
    .map(|v| v.trim_start_matches("1.").to_string())
    .filter(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()));
    let main_class = tag(&all, "mainClass").map(str::to_string);
    BuildProject {
        tool: BuildTool::Maven,
        name,
        wrapper: files.iter().any(|f| f == "mvnw" || f == "mvnw.cmd"),
        build_file: "pom.xml".into(),
        settings_file: None,
        kotlin_dsl: false,
        modules,
        jacoco: all.contains("jacoco-maven-plugin"),
        application: main_class.is_some(),
        main_class,
        spring_boot: all.contains("spring-boot"),
        custom_tasks: Vec::new(),
        java_version,
        test_frameworks: test_frameworks(&all, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn project(files: &[(&str, &str)], prefer: Option<BuildTool>) -> Option<BuildProject> {
        let map: HashMap<String, String> = files
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let names: Vec<String> = files
            .iter()
            .map(|(k, _)| k.to_string())
            .filter(|k| !k.contains('/'))
            .collect();
        detect("shop", &names, &|p| map.get(p).cloned(), prefer)
    }

    #[test]
    fn gradle_kotlin_multi_project() {
        let p = project(
            &[
                ("gradlew", ""),
                (
                    "settings.gradle.kts",
                    "rootProject.name = \"inventory\"\ninclude(\n    \"app\",\n    \"core:util\", // shared\n)\ninclude(\":api\")\n",
                ),
                (
                    "build.gradle.kts",
                    "plugins {\n    java\n    application\n    jacoco\n}\njava { toolchain { languageVersion = JavaLanguageVersion.of(21) } }\napplication { mainClass = \"com.acme.App\" }\ntasks.register<Copy>(\"bundle\") {}\ntasks.test { useJUnitPlatform() }\n",
                ),
                ("app/build.gradle.kts", "dependencies { testImplementation(\"org.testng:testng:7.10.2\") }"),
            ],
            None,
        )
        .unwrap();
        assert_eq!(p.tool, BuildTool::Gradle);
        assert_eq!(p.name, "inventory");
        assert!(p.wrapper && p.kotlin_dsl && p.jacoco && p.application);
        assert_eq!(p.main_class.as_deref(), Some("com.acme.App"));
        assert_eq!(p.java_version.as_deref(), Some("21"));
        assert_eq!(p.custom_tasks, vec!["bundle"]);
        let paths: Vec<&str> = p.modules.iter().map(|m| m.path.as_str()).collect();
        assert_eq!(paths, [":api", ":app", ":core:util"]);
        assert_eq!(p.module("core:util").unwrap().dir, "core/util");
        assert_eq!(
            p.module_of("core/util/src/test/java/A.java")
                .map(|m| m.path.as_str()),
            Some(":core:util")
        );
        assert_eq!(
            p.test_frameworks,
            [TestFrameworkUse::JUnit5, TestFrameworkUse::TestNg]
        );
    }

    #[test]
    fn gradle_groovy_single_project() {
        let p = project(
            &[(
                "build.gradle",
                "apply plugin: 'java'\nid 'org.springframework.boot' version '3.3.0'\nsourceCompatibility = '17'\ntask hello { }\ndependencies { testImplementation 'junit:junit:4.13.2' }\nmainClassName = 'demo.Main'",
            )],
            None,
        )
        .unwrap();
        assert!(!p.wrapper && !p.kotlin_dsl && !p.jacoco);
        assert!(p.spring_boot);
        assert_eq!(p.name, "shop");
        assert_eq!(p.java_version.as_deref(), Some("17"));
        assert_eq!(p.custom_tasks, vec!["hello"]);
        assert_eq!(p.main_class.as_deref(), Some("demo.Main"));
        assert_eq!(p.test_frameworks, [TestFrameworkUse::JUnit4]);
    }

    #[test]
    fn maven_modules_and_plugins() {
        let pom = r#"<project>
  <parent><artifactId>spring-boot-starter-parent</artifactId></parent>
  <artifactId>shop-parent</artifactId>
  <properties><maven.compiler.release>17</maven.compiler.release></properties>
  <modules>
    <module>web</module>
    <module>domain/</module>
    <!-- <module>legacy</module> -->
  </modules>
  <build><plugins><plugin><artifactId>jacoco-maven-plugin</artifactId></plugin></plugins></build>
</project>"#;
        let web = "<project><dependencies><dependency><groupId>org.junit.jupiter</groupId><artifactId>junit-jupiter</artifactId></dependency></dependencies></project>";
        let p = project(
            &[("pom.xml", pom), ("mvnw", ""), ("web/pom.xml", web)],
            None,
        )
        .unwrap();
        assert_eq!(p.tool, BuildTool::Maven);
        assert_eq!(p.name, "shop-parent");
        assert!(p.wrapper && p.jacoco && p.spring_boot);
        assert_eq!(p.java_version.as_deref(), Some("17"));
        let dirs: Vec<&str> = p.modules.iter().map(|m| m.dir.as_str()).collect();
        assert_eq!(dirs, ["web", "domain"]);
        assert_eq!(p.test_frameworks, [TestFrameworkUse::JUnit5]);
    }

    #[test]
    fn both_builds_and_none() {
        let files = [("pom.xml", "<project/>"), ("build.gradle", "")];
        assert_eq!(project(&files, None).unwrap().tool, BuildTool::Gradle);
        assert_eq!(
            project(&files, Some(BuildTool::Maven)).unwrap().tool,
            BuildTool::Maven
        );
        assert!(project(&[("Main.java", "")], None).is_none());
        assert_eq!(BuildTool::parse("Maven"), Some(BuildTool::Maven));
    }
}
