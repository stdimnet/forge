//! The "Minecraft Mod" New Project generator: Fabric (Loom) and NeoForge (ModDevGradle) mods
//! with the Gradle build and `gradle.properties`, `fabric.mod.json` or
//! `META-INF/neoforge.mods.toml`, the main class and the client (or server) initializer, an
//! example item and block with their models, translations and textures, mixins, data
//! generation, `.gitignore`, a README and the "Minecraft Client" / "Minecraft Server" run
//! configurations. Nothing here touches the network.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::generate::{class_name, is_qualified_name, package_segment, GeneratedFile};
use crate::mods::png;
use crate::mods::sources::{self as src, fill, Api};
use crate::mods::versions::{self, is_unobfuscated, Loader, Resolved};
use crate::versions::{compare, KOTLIN};

/// The wizard's options (all strings and booleans, as the New Project dialog sends them).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModOptions {
    /// `fabric` or `neoforge`.
    pub loader: String,
    pub mc_version: String,
    /// Fabric Loader or NeoForge version; empty: the newest known for the Minecraft version.
    pub loader_version: String,
    /// Fabric on 1.21.x: `mojang` (official Mojang mappings) or `yarn`.
    pub fabric_mappings: String,
    /// NeoForge on 1.21.x: `parchment` (Mojang's names with parameter names) or `official`.
    pub neo_mappings: String,
    /// Empty: from the project name (`better-chests` → `better_chests`).
    pub mod_id: String,
    /// Empty: from the project name (`Better Chests`).
    pub mod_name: String,
    pub mod_version: String,
    pub group_id: String,
    /// Comma-separated.
    pub authors: String,
    pub description: String,
    pub license: String,
    /// `java` or `kotlin` (Fabric Language Kotlin).
    pub language: String,
    pub mixins: bool,
    /// `both`, `client` or `server`: which initializers and the mod's environment.
    pub environment: String,
    pub datagen: bool,
}

impl Default for ModOptions {
    fn default() -> Self {
        Self {
            loader: "fabric".into(),
            mc_version: versions::DEFAULT_MOD_MC_VERSION.into(),
            loader_version: String::new(),
            fabric_mappings: "mojang".into(),
            neo_mappings: "parchment".into(),
            mod_id: String::new(),
            mod_name: String::new(),
            mod_version: "1.0.0".into(),
            group_id: "com.example".into(),
            authors: String::new(),
            description: String::new(),
            license: "All Rights Reserved".into(),
            language: "java".into(),
            mixins: true,
            environment: "both".into(),
            datagen: false,
        }
    }
}

impl ModOptions {
    /// Options from the wizard's JSON values; unknown keys (`git`) are ignored.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, String> {
        serde_json::from_value(value.clone()).map_err(|e| e.to_string())
    }
}

/// How the build maps Minecraft's names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "version")]
pub enum Mappings {
    /// 26.1+: the game ships with Mojang's names.
    None,
    /// Official Mojang mappings (Fabric `loom.officialMojangMappings()`, NeoForge's default).
    Mojang,
    Yarn(String),
    Parchment(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Environment {
    Both,
    Client,
    Server,
}

impl Environment {
    pub fn has_client(self) -> bool {
        self != Environment::Server
    }
}

/// Everything the files are generated from, after validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModProject {
    /// The project folder name (Gradle root project).
    pub name: String,
    pub loader: Loader,
    pub versions: Resolved,
    /// Fabric Loader or NeoForge version.
    pub loader_version: String,
    pub mappings: Mappings,
    pub kotlin: bool,
    pub mixins: bool,
    pub environment: Environment,
    pub datagen: bool,
    pub mod_id: String,
    pub mod_name: String,
    pub mod_version: String,
    pub group_id: String,
    pub package: String,
    pub class: String,
    pub authors: Vec<String>,
    pub description: String,
    pub license: String,
}

impl ModProject {
    pub fn mc(&self) -> &str {
        &self.versions.mc_version
    }

    fn at_least(&self, v: &str) -> bool {
        compare(self.mc(), v) != std::cmp::Ordering::Less
    }

    /// The names the sources use: Yarn only for Fabric with Yarn mappings.
    fn api(&self) -> Api {
        match self.mappings {
            Mappings::Yarn(_) => Api::yarn(),
            _ => Api::mojang(self.at_least("1.21.11")),
        }
    }

    /// Items and blocks carry their registry key in their properties from 1.21.2 on.
    fn registry_keys(&self) -> bool {
        self.at_least("1.21.2")
    }

    /// Item model definitions (`assets/<mod>/items/`) exist from 1.21.4 on.
    fn item_definitions(&self) -> bool {
        self.at_least("1.21.4")
    }

    /// Fabric splits the client sources into `src/client` when the mod has a client side.
    fn split_sources(&self) -> bool {
        self.loader == Loader::Fabric && self.environment.has_client()
    }

    fn compatibility_level(&self) -> String {
        format!("JAVA_{}", self.versions.java)
    }
}

/// A mod id: `[a-z][a-z0-9_]{1,63}` (NeoForge's rule; Fabric also allows `-`).
pub fn is_mod_id(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && (2..=64).contains(&s.len())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The derived mod id of a project (`Better-Chests 2` → `better_chests_2`).
pub fn mod_id_of(project: &str) -> String {
    let words: Vec<String> = project
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .flat_map(split_camel)
        .map(|w| w.to_ascii_lowercase())
        .collect();
    let id = words.join("_");
    let id = if id.starts_with(|c: char| c.is_ascii_lowercase()) {
        id
    } else {
        format!("mod_{id}").trim_end_matches('_').to_string()
    };
    let id: String = id.chars().take(64).collect();
    if id.len() < 2 {
        "mymod".into()
    } else {
        id
    }
}

fn split_camel(word: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for ch in word.chars() {
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

/// The display name from a mod id (`better_chests` → `Better Chests`).
pub fn display_name(mod_id: &str) -> String {
    mod_id
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Validate the options and derive the names and versions.
pub fn resolve(project: &str, o: &ModOptions) -> Result<ModProject, String> {
    let loader = Loader::parse(o.loader.trim())
        .ok_or_else(|| format!("Unknown mod loader '{}'", o.loader))?;
    let mc = o.mc_version.trim();
    let versions = versions::resolve_versions(mc)?;
    let kotlin = match o.language.trim() {
        "" | "java" => false,
        "kotlin" if loader == Loader::Fabric => true,
        "kotlin" => {
            return Err("Kotlin mods are generated for Fabric (Fabric Language Kotlin)".into())
        }
        other => return Err(format!("Unknown language '{other}'")),
    };
    let loader_version = match (loader, o.loader_version.trim()) {
        (Loader::Fabric, "") => versions.fabric_loader.clone(),
        (Loader::Neoforge, "") => versions.neoforge.clone().ok_or_else(|| {
            format!("No NeoForge version is known for Minecraft {mc}: click Refresh next to Minecraft, or enter one")
        })?,
        (Loader::Fabric, v) => {
            if !v.split('.').all(|p| p.parse::<u64>().is_ok()) {
                return Err(format!("'{v}' is not a Fabric Loader version such as {}", versions::FABRIC_LOADER));
            }
            v.to_string()
        }
        (Loader::Neoforge, v) => {
            if versions::neoforge_mc_version(v).as_deref() != Some(mc) {
                return Err(format!("NeoForge {v} is not a version for Minecraft {mc}"));
            }
            v.to_string()
        }
    };
    if loader == Loader::Fabric && versions.fabric_api.is_none() {
        return Err(format!(
            "No Fabric API version is known for Minecraft {mc}: click Refresh next to Minecraft"
        ));
    }
    let mappings = if is_unobfuscated(mc) {
        Mappings::None
    } else {
        match (loader, o.fabric_mappings.trim(), o.neo_mappings.trim()) {
            (Loader::Fabric, "yarn", _) => Mappings::Yarn(versions.yarn.clone().ok_or_else(|| {
                format!("No Yarn mappings are known for Minecraft {mc}: click Refresh, or use Mojang mappings")
            })?),
            (Loader::Fabric, "" | "mojang", _) => Mappings::Mojang,
            (Loader::Neoforge, _, "" | "parchment") => {
                Mappings::Parchment(versions.parchment.clone().ok_or_else(|| {
                    format!("No Parchment mappings are known for Minecraft {mc}: use the official mappings")
                })?)
            }
            (Loader::Neoforge, _, "official") => Mappings::Mojang,
            (_, f, n) => {
                return Err(format!(
                    "Unknown mappings '{}'",
                    if loader == Loader::Fabric { f } else { n }
                ))
            }
        }
    };
    let environment = match o.environment.trim() {
        "" | "both" => Environment::Both,
        "client" => Environment::Client,
        "server" => Environment::Server,
        other => return Err(format!("Unknown environment '{other}'")),
    };
    let mod_id = match o.mod_id.trim() {
        "" => mod_id_of(project),
        id => id.to_string(),
    };
    if !is_mod_id(&mod_id) {
        return Err(
            "Mod ID must be 2 to 64 lowercase letters, digits and '_', starting with a letter"
                .into(),
        );
    }
    if [
        "minecraft",
        "fabric",
        "fabricloader",
        "neoforge",
        "java",
        "forge",
    ]
    .contains(&mod_id.as_str())
    {
        return Err(format!("The mod ID '{mod_id}' is reserved"));
    }
    let group_id = o.group_id.trim().to_string();
    if !is_qualified_name(&group_id) {
        return Err("Group ID must be a Java package name such as com.example".into());
    }
    if group_id.starts_with("net.minecraft") {
        return Err("The group ID must not be Minecraft's package".into());
    }
    let mod_name = match o.mod_name.trim() {
        "" => display_name(&mod_id),
        n => n.to_string(),
    };
    let mod_version = match o.mod_version.trim() {
        "" => "1.0.0".to_string(),
        v if v
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+')) =>
        {
            v.to_string()
        }
        v => {
            return Err(format!(
                "'{v}' is not a mod version such as 1.0.0 (letters, digits, '.', '-', '+')"
            ))
        }
    };
    let license = match o.license.trim() {
        "" => "All Rights Reserved".to_string(),
        l => l.to_string(),
    };
    let authors = o
        .authors
        .split(',')
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_string)
        .collect();
    Ok(ModProject {
        name: project.trim().to_string(),
        loader,
        versions,
        loader_version,
        mappings,
        kotlin,
        mixins: o.mixins,
        environment,
        datagen: o.datagen,
        package: format!("{group_id}.{}", package_segment(&mod_id)),
        class: class_name(&mod_id),
        mod_id,
        mod_name,
        mod_version,
        group_id,
        authors,
        description: o.description.trim().to_string(),
        license,
    })
}

/// Generate the mod's files (sorted by path).
pub fn generate_mod(project: &str, o: &ModOptions) -> Result<Vec<GeneratedFile>, String> {
    Ok(mod_files(&resolve(project, o)?))
}

fn text(path: impl Into<String>, contents: impl Into<String>) -> GeneratedFile {
    GeneratedFile {
        path: path.into(),
        contents: contents.into(),
        binary: None,
    }
}

fn binary(path: impl Into<String>, bytes: Vec<u8>) -> GeneratedFile {
    GeneratedFile {
        path: path.into(),
        contents: String::new(),
        binary: Some(bytes),
    }
}

/// The files of a resolved mod (sorted by path).
pub fn mod_files(p: &ModProject) -> Vec<GeneratedFile> {
    let mut out = Vec::new();
    out.push(text("settings.gradle", settings_gradle(p)));
    out.push(text("gradle.properties", gradle_properties(p)));
    out.push(text(
        "gradle/gradle-daemon-jvm.properties",
        versions::DAEMON_JVM_PROPERTIES,
    ));
    match p.loader {
        Loader::Fabric => {
            out.push(text("build.gradle", fabric_build(p)));
            out.push(text(
                "src/main/resources/fabric.mod.json",
                fabric_mod_json(p),
            ));
            fabric_sources(p, &mut out);
        }
        Loader::Neoforge => {
            out.push(text("build.gradle", neoforge_build(p)));
            out.push(text(
                "src/main/templates/META-INF/neoforge.mods.toml",
                neoforge_mods_toml(p),
            ));
            neoforge_sources(p, &mut out);
        }
    }
    assets(p, &mut out);
    out.push(text(".gitignore", gitignore(p)));
    out.push(text("README.md", readme(p)));
    out.push(text(".forge/runConfigurations.toml", run_configurations(p)));
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

// ---- Names and escaping -----------------------------------------------------------------------

fn vars(p: &ModProject) -> BTreeMap<&'static str, String> {
    let mut v = BTreeMap::new();
    v.insert("package", p.package.clone());
    v.insert("class", p.class.clone());
    v.insert("modid", p.mod_id.clone());
    v.insert("name", p.mod_name.replace(['"', '\\', '\n'], ""));
    v
}

/// Text in a resource Gradle expands: `$` starts a template expression.
fn literal(s: &str) -> String {
    s.replace('$', "\\$")
}

fn json_str(s: &str) -> String {
    literal(&serde_json::to_string(s).unwrap_or_default())
}

fn toml_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    literal(&out)
}

fn lang_dir(p: &ModProject) -> (&'static str, &'static str) {
    if p.kotlin {
        ("kotlin", "kt")
    } else {
        ("java", "java")
    }
}

/// The source set the client (and Fabric's data generator) lives in.
fn client_set(p: &ModProject) -> &'static str {
    if p.split_sources() {
        "client"
    } else {
        "main"
    }
}

fn pkg_dir(package: &str) -> String {
    package.replace('.', "/")
}

/// The fully qualified client initializer, if the mod has one.
pub fn client_class(p: &ModProject) -> Option<String> {
    p.environment.has_client().then(|| match p.loader {
        Loader::Fabric => format!("{}.client.{}Client", p.package, p.class),
        Loader::Neoforge => format!("{}.{}Client", p.package, p.class),
    })
}

fn datagen_package(p: &ModProject) -> String {
    match p.loader {
        Loader::Fabric if p.split_sources() => format!("{}.client.datagen", p.package),
        _ => format!("{}.datagen", p.package),
    }
}

fn mixin_config(p: &ModProject) -> String {
    format!("{}.mixins.json", p.mod_id)
}

fn client_mixin_config(p: &ModProject) -> String {
    format!("{}.client.mixins.json", p.mod_id)
}

// ---- Gradle -----------------------------------------------------------------------------------

fn settings_gradle(p: &ModProject) -> String {
    let repos = match p.loader {
        Loader::Fabric => format!(
            "        maven {{\n            name = 'Fabric'\n            url = '{}'\n        }}\n        mavenCentral()\n        gradlePluginPortal()\n",
            versions::FABRIC_MAVEN
        ),
        Loader::Neoforge => "        gradlePluginPortal()\n".to_string(),
    };
    format!(
        "pluginManagement {{\n    repositories {{\n{repos}    }}\n}}\n\nplugins {{\n    // Downloads the JDK of the project's Java release when it is not installed.\n    id 'org.gradle.toolchains.foojay-resolver-convention' version '{}'\n}}\n\nrootProject.name = '{}'\n",
        crate::versions::FOOJAY_RESOLVER,
        p.name.replace(['\'', '\\'], "")
    )
}

fn gradle_properties(p: &ModProject) -> String {
    let v = &p.versions;
    let mut s = String::from("# Gradle\norg.gradle.jvmargs=-Xmx2G\norg.gradle.parallel=true\n\n");
    match p.loader {
        Loader::Fabric => {
            s += "# Fabric: see https://fabricmc.net/develop for newer versions\n";
            s += &format!("minecraft_version={}\n", p.mc());
            s += &format!("loader_version={}\n", p.loader_version);
            s += &format!("loom_version={}\n", v.loom);
            if let Mappings::Yarn(y) = &p.mappings {
                s += &format!("yarn_mappings={y}\n");
            }
            s += &format!(
                "fabric_api_version={}\n",
                v.fabric_api.clone().unwrap_or_default()
            );
            if p.kotlin {
                s += &format!("fabric_kotlin_version={}\n", versions::FABRIC_KOTLIN);
            }
            s += "\n# Mod\n";
            s += &format!("mod_version={}\n", p.mod_version);
            s += &format!("maven_group={}\n", p.group_id);
            s += &format!("archives_base_name={}\n", p.mod_id);
        }
        Loader::Neoforge => {
            s += "# NeoForge: see https://projects.neoforged.net/neoforged/neoforge for newer versions\n";
            s += &format!("minecraft_version={}\n", p.mc());
            s += &format!("minecraft_version_range=[{}]\n", p.mc());
            s += &format!("neo_version={}\n", p.loader_version);
            if let Mappings::Parchment(pv) = &p.mappings {
                s += &format!("parchment_minecraft_version={}\n", p.mc());
                s += &format!("parchment_mappings_version={pv}\n");
            }
            s += "\n# Mod\n";
            s += &format!("mod_id={}\n", p.mod_id);
            s += &format!("mod_version={}\n", p.mod_version);
            s += &format!("mod_group_id={}\n", p.group_id);
        }
    }
    s
}

fn fabric_build(p: &ModProject) -> String {
    let java = p.versions.java;
    let remap = !matches!(p.mappings, Mappings::None);
    let plugin = if remap {
        "fabric-loom"
    } else {
        "net.fabricmc.fabric-loom"
    };
    let modconf = if remap {
        "modImplementation"
    } else {
        "implementation"
    };
    let mut s = String::from("plugins {\n");
    s += &format!("    id '{plugin}' version \"${{loom_version}}\"\n");
    if p.kotlin {
        s += &format!("    id 'org.jetbrains.kotlin.jvm' version '{KOTLIN}'\n");
    }
    s += "}\n\nversion = project.mod_version\ngroup = project.maven_group\n\nbase {\n    archivesName = project.archives_base_name\n}\n\n";
    s += "repositories {\n    // Loom adds the repositories of Minecraft, Fabric and their libraries. Add the\n    // repositories of other mods you depend on here.\n}\n\n";
    s += "loom {\n";
    if p.split_sources() {
        s += "    // Client code lives in src/client and cannot be loaded on a dedicated server.\n    splitEnvironmentSourceSets()\n\n";
    }
    s += &format!(
        "    mods {{\n        '{}' {{\n            sourceSet sourceSets.main\n",
        p.mod_id
    );
    if p.split_sources() {
        s += "            sourceSet sourceSets.client\n";
    }
    s += "        }\n    }\n}\n\n";
    if p.datagen {
        if p.split_sources() {
            s += "fabricApi {\n    // runDatagen writes the generated resources to src/main/generated.\n    configureDataGeneration {\n        client = true\n    }\n}\n\n";
        } else {
            s += "fabricApi {\n    // runDatagen writes the generated resources to src/main/generated.\n    configureDataGeneration()\n}\n\n";
        }
    }
    s += "dependencies {\n    minecraft \"com.mojang:minecraft:${project.minecraft_version}\"\n";
    match &p.mappings {
        Mappings::Yarn(_) => {
            s += "    mappings \"net.fabricmc:yarn:${project.yarn_mappings}:v2\"\n"
        }
        Mappings::Mojang => s += "    mappings loom.officialMojangMappings()\n",
        _ => {}
    }
    s += &format!("    {modconf} \"net.fabricmc:fabric-loader:${{project.loader_version}}\"\n");
    s += &format!(
        "    {modconf} \"net.fabricmc.fabric-api:fabric-api:${{project.fabric_api_version}}\"\n"
    );
    if p.kotlin {
        s += &format!(
            "    {modconf} \"net.fabricmc:fabric-language-kotlin:${{project.fabric_kotlin_version}}\"\n"
        );
    }
    s += "}\n\n";
    s += "processResources {\n    def version = project.version\n    inputs.property 'version', version\n\n    filesMatching('fabric.mod.json') {\n        expand 'version': version\n    }\n}\n\n";
    s += &format!(
        "java {{\n    toolchain.languageVersion = JavaLanguageVersion.of({java})\n    withSourcesJar()\n}}\n\ntasks.withType(JavaCompile).configureEach {{\n    options.encoding = 'UTF-8'\n    options.release = {java}\n}}\n"
    );
    s
}

fn neoforge_build(p: &ModProject) -> String {
    let java = p.versions.java;
    let mut s = format!(
        "plugins {{\n    id 'java-library'\n    id 'net.neoforged.moddev' version '{}'\n}}\n\n",
        p.versions.moddev
    );
    s += "version = mod_version\ngroup = mod_group_id\n\nbase {\n    archivesName = mod_id\n}\n\n";
    s += &format!("java.toolchain.languageVersion = JavaLanguageVersion.of({java})\n\n");
    if p.datagen {
        s += "sourceSets.main.resources {\n    // Resources of the data generators (runData).\n    srcDir 'src/generated/resources'\n    exclude '.cache'\n}\n\n";
    }
    s += "repositories {\n    // Add the repositories of other mods you depend on here.\n}\n\n";
    s += "neoForge {\n    version = project.neo_version\n\n";
    if let Mappings::Parchment(_) = p.mappings {
        s += "    // Parameter names and javadoc on top of Mojang's names.\n    parchment {\n        mappingsVersion = project.parchment_mappings_version\n        minecraftVersion = project.parchment_minecraft_version\n    }\n\n";
    }
    s += "    runs {\n";
    s += "        client {\n            client()\n            systemProperty 'neoforge.enabledGameTestNamespaces', project.mod_id\n        }\n\n";
    s += "        server {\n            server()\n            programArgument '--nogui'\n            systemProperty 'neoforge.enabledGameTestNamespaces', project.mod_id\n        }\n\n";
    if p.datagen {
        let data = if p.at_least("1.21.4") {
            "clientData()"
        } else {
            "data()"
        };
        s += &format!(
            "        data {{\n            {data}\n            programArguments.addAll '--mod', project.mod_id, '--all', '--output', file('src/generated/resources/').getAbsolutePath(), '--existing', file('src/main/resources/').getAbsolutePath()\n        }}\n\n"
        );
    }
    s += "        configureEach {\n            systemProperty 'forge.logging.markers', 'REGISTRIES'\n            logLevel = org.slf4j.event.Level.DEBUG\n        }\n    }\n\n";
    s += "    mods {\n        \"${mod_id}\" {\n            sourceSet(sourceSets.main)\n        }\n    }\n}\n\n";
    s += "// Fills the version properties into src/main/templates (META-INF/neoforge.mods.toml).\nvar generateModMetadata = tasks.register('generateModMetadata', ProcessResources) {\n    var replaceProperties = [\n            minecraft_version      : minecraft_version,\n            minecraft_version_range: minecraft_version_range,\n            neo_version            : neo_version,\n            mod_id                 : mod_id,\n            mod_version            : mod_version,\n    ]\n    inputs.properties replaceProperties\n    expand replaceProperties\n    from 'src/main/templates'\n    into 'build/generated/sources/modMetadata'\n}\nsourceSets.main.resources.srcDir generateModMetadata\nneoForge.ideSyncTask generateModMetadata\n\n";
    s += "tasks.withType(JavaCompile).configureEach {\n    options.encoding = 'UTF-8'\n}\n";
    s
}

// ---- Descriptors ------------------------------------------------------------------------------

fn fabric_mod_json(p: &ModProject) -> String {
    let mut s = String::from("{\n  \"schemaVersion\": 1,\n");
    s += &format!("  \"id\": \"{}\",\n", p.mod_id);
    s += "  \"version\": \"${version}\",\n";
    s += &format!("  \"name\": {},\n", json_str(&p.mod_name));
    s += &format!("  \"description\": {},\n", json_str(&p.description));
    let authors = p
        .authors
        .iter()
        .map(|a| json_str(a))
        .collect::<Vec<_>>()
        .join(", ");
    s += &format!("  \"authors\": [{authors}],\n");
    s += "  \"contact\": {},\n";
    s += &format!("  \"license\": {},\n", json_str(&p.license));
    s += &format!("  \"icon\": \"assets/{}/icon.png\",\n", p.mod_id);
    let env = match p.environment {
        Environment::Both => "*",
        Environment::Client => "client",
        Environment::Server => "server",
    };
    s += &format!("  \"environment\": \"{env}\",\n");
    let mut entrypoints = vec![format!(
        "    \"main\": [\n      \"{}.{}\"\n    ]",
        p.package, p.class
    )];
    if let Some(client) = client_class(p) {
        entrypoints.push(format!("    \"client\": [\n      \"{client}\"\n    ]"));
    }
    if p.environment == Environment::Server {
        entrypoints.push(format!(
            "    \"server\": [\n      \"{}.{}Server\"\n    ]",
            p.package, p.class
        ));
    }
    if p.datagen {
        entrypoints.push(format!(
            "    \"fabric-datagen\": [\n      \"{}.{}DataGenerator\"\n    ]",
            datagen_package(p),
            p.class
        ));
    }
    s += &format!(
        "  \"entrypoints\": {{\n{}\n  }},\n",
        entrypoints.join(",\n")
    );
    if p.mixins {
        s += &format!("  \"mixins\": [\n    \"{}\"", mixin_config(p));
        if p.environment.has_client() {
            s += &format!(
                ",\n    {{\n      \"config\": \"{}\",\n      \"environment\": \"client\"\n    }}",
                client_mixin_config(p)
            );
        }
        s += "\n  ],\n";
    }
    let minecraft = format!("~{}", p.mc());
    s += "  \"depends\": {\n";
    s += &format!("    \"fabricloader\": \">={}\",\n", p.loader_version);
    s += &format!("    \"minecraft\": \"{minecraft}\",\n");
    s += &format!("    \"java\": \">={}\",\n", p.versions.java);
    s += "    \"fabric-api\": \"*\"";
    if p.kotlin {
        s += ",\n    \"fabric-language-kotlin\": \"*\"";
    }
    s += "\n  }\n}\n";
    s
}

fn neoforge_mods_toml(p: &ModProject) -> String {
    let mut s = String::from(
        "# The mod's metadata. Gradle fills in the properties from gradle.properties\n# (generateModMetadata). See https://docs.neoforged.net/docs/gettingstarted/modfiles\nmodLoader = \"javafml\"\nloaderVersion = \"[1,)\"\n",
    );
    s += &format!("license = {}\n\n", toml_str(&p.license));
    s += "[[mods]]\nmodId = \"${mod_id}\"\nversion = \"${mod_version}\"\n";
    s += &format!("displayName = {}\n", toml_str(&p.mod_name));
    if !p.authors.is_empty() {
        s += &format!("authors = {}\n", toml_str(&p.authors.join(", ")));
    }
    s += &format!("description = {}\n", toml_str(&p.description));
    match p.environment {
        Environment::Both => {}
        Environment::Client => {
            s += "# A client-side mod: servers do not need it.\ndisplayTest = \"IGNORE_ALL_VERSION\"\n"
        }
        Environment::Server => {
            s += "# A server-side mod: clients do not need it.\ndisplayTest = \"IGNORE_SERVER_VERSION\"\n"
        }
    }
    if p.mixins {
        s += "\n[[mixins]]\nconfig = \"${mod_id}.mixins.json\"\n";
    }
    s += "\n[[dependencies.${mod_id}]]\nmodId = \"neoforge\"\ntype = \"required\"\nversionRange = \"[${neo_version},)\"\nordering = \"NONE\"\nside = \"BOTH\"\n";
    s += "\n[[dependencies.${mod_id}]]\nmodId = \"minecraft\"\ntype = \"required\"\nversionRange = \"${minecraft_version_range}\"\nordering = \"NONE\"\nside = \"BOTH\"\n";
    s
}

fn mixin_json(p: &ModProject, package: &str, key: &str, classes: &[&str]) -> String {
    let list = classes
        .iter()
        .map(|c| format!("    \"{c}\""))
        .collect::<Vec<_>>()
        .join(",\n");
    let mut s = String::from("{\n  \"required\": true,\n  \"minVersion\": \"0.8\",\n");
    s += &format!("  \"package\": \"{package}\",\n");
    s += &format!(
        "  \"compatibilityLevel\": \"{}\",\n",
        p.compatibility_level()
    );
    s += &format!("  \"{key}\": [\n{list}\n  ],\n");
    s += "  \"injectors\": {\n    \"defaultRequire\": 1\n  },\n  \"overwrites\": {\n    \"requireAnnotations\": true\n  }\n}\n";
    s
}

// ---- Sources ----------------------------------------------------------------------------------

/// Import lines (Java with `;`, Kotlin without).
fn imports(list: &[&str], kotlin: bool) -> String {
    let semi = if kotlin { "" } else { ";" };
    list.iter()
        .map(|i| format!("import {i}{semi}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn fabric_sources(p: &ModProject, out: &mut Vec<GeneratedFile>) {
    let api = p.api();
    let (dir, ext) = lang_dir(p);
    let kt = p.kotlin;
    let mut v = vars(p);
    let keys = p.registry_keys();
    let mut item_imports = vec![api.registry_import, api.builtin_import];
    if keys {
        item_imports.extend([api.keys_import, api.key_import]);
    }
    item_imports.push(api.item_import);
    let mut block_imports = item_imports.clone();
    block_imports.extend([
        api.block_item_import,
        api.block_import,
        api.block_props_import,
        api.sound_import,
    ]);
    v.insert("item_imports", imports(&item_imports, kt));
    v.insert("block_imports", imports(&block_imports, kt));
    let (reg_item, reg_block) = match (kt, keys) {
        (false, true) => (
            "        {{key}}<Item> key = {{key_of}}({{keys}}.ITEM, {{class}}.id(name));\n        return Registry.register({{builtin}}.ITEM, key, factory.apply(properties.{{set_id}}(key)));",
            "        {{key}}<Block> key = {{key_of}}({{keys}}.BLOCK, {{class}}.id(name));\n        Block block = Registry.register({{builtin}}.BLOCK, key, factory.apply(properties.{{set_id}}(key)));",
        ),
        (false, false) => (
            "        return Registry.register({{builtin}}.ITEM, {{class}}.id(name), factory.apply(properties));",
            "        Block block = Registry.register({{builtin}}.BLOCK, {{class}}.id(name), factory.apply(properties));",
        ),
        (true, true) => (
            "        val key = {{key_of}}({{keys}}.ITEM, {{class}}.id(name))\n        return Registry.register({{builtin}}.ITEM, key, factory(properties.{{set_id}}(key)))",
            "        val key = {{key_of}}({{keys}}.BLOCK, {{class}}.id(name))\n        val block = Registry.register({{builtin}}.BLOCK, key, factory(properties.{{set_id}}(key)))",
        ),
        (true, false) => (
            "        return Registry.register({{builtin}}.ITEM, {{class}}.id(name), factory(properties))",
            "        val block = Registry.register({{builtin}}.BLOCK, {{class}}.id(name), factory(properties))",
        ),
    };
    v.insert("register_item", reg_item.into());
    v.insert("register_block", reg_block.into());
    v.insert(
        "block_item_prefix",
        if keys {
            format!(".{}", api.block_prefix)
        } else {
            String::new()
        },
    );
    v.insert("datagen_package", datagen_package(p));
    v.insert(
        "client_mixin_package",
        format!("{}.client.mixin", p.package),
    );
    // Two passes: the injected bodies contain placeholders themselves.
    let render = |t: &str| fill(&fill(t, &v, Some(&api)), &v, Some(&api));
    let main_dir = format!("src/main/{dir}/{}", pkg_dir(&p.package));
    let client_root = format!("src/{}/{dir}", client_set(p));
    let pick = |java: &'static str, kotlin: &'static str| if kt { kotlin } else { java };
    out.push(text(
        format!("{main_dir}/{}.{ext}", p.class),
        render(pick(src::FABRIC_MAIN, src::FABRIC_KT_MAIN)),
    ));
    out.push(text(
        format!("{main_dir}/registry/ModItems.{ext}"),
        render(pick(src::FABRIC_ITEMS, src::FABRIC_KT_ITEMS)),
    ));
    out.push(text(
        format!("{main_dir}/registry/ModBlocks.{ext}"),
        render(pick(src::FABRIC_BLOCKS, src::FABRIC_KT_BLOCKS)),
    ));
    if let Some(client) = client_class(p) {
        out.push(text(
            format!("{client_root}/{}.{ext}", pkg_dir(&client)),
            render(pick(src::FABRIC_CLIENT, src::FABRIC_KT_CLIENT)),
        ));
    } else {
        out.push(text(
            format!("{main_dir}/{}Server.{ext}", p.class),
            render(pick(src::FABRIC_SERVER, src::FABRIC_KT_SERVER)),
        ));
    }
    if p.datagen {
        out.push(text(
            format!(
                "{client_root}/{}/{}DataGenerator.{ext}",
                pkg_dir(&datagen_package(p)),
                p.class
            ),
            render(pick(src::FABRIC_DATAGEN, src::FABRIC_KT_DATAGEN)),
        ));
    }
    if p.mixins {
        // Mixins are Java classes, also in Kotlin mods.
        let mixin_pkg = format!("{}.mixin", p.package);
        out.push(text(
            format!("src/main/java/{}/ExampleMixin.java", pkg_dir(&mixin_pkg)),
            render(src::FABRIC_MIXIN),
        ));
        out.push(text(
            format!("src/main/resources/{}", mixin_config(p)),
            mixin_json(p, &mixin_pkg, "mixins", &["ExampleMixin"]),
        ));
        if p.environment.has_client() {
            let client_pkg = format!("{}.client.mixin", p.package);
            out.push(text(
                format!(
                    "src/client/java/{}/ExampleClientMixin.java",
                    pkg_dir(&client_pkg)
                ),
                render(src::CLIENT_MIXIN),
            ));
            out.push(text(
                format!("src/client/resources/{}", client_mixin_config(p)),
                mixin_json(p, &client_pkg, "client", &["ExampleClientMixin"]),
            ));
        }
    }
}

fn neoforge_sources(p: &ModProject, out: &mut Vec<GeneratedFile>) {
    let api = p.api();
    let mut v = vars(p);
    let (mut extra_imports, mut extra_listeners) = (String::new(), String::new());
    if p.datagen {
        extra_imports += &format!("import {}.datagen.ModDataGenerators;\n", p.package);
        extra_listeners += "        modEventBus.addListener(ModDataGenerators::gatherData);\n";
    }
    v.insert("extra_imports", extra_imports);
    v.insert("extra_listeners", extra_listeners);
    let lambda = p.at_least("1.21.10");
    v.insert(
        "props_import",
        if lambda {
            String::new()
        } else {
            "import net.minecraft.world.level.block.state.BlockBehaviour;\n".into()
        },
    );
    v.insert(
        "block_properties",
        if lambda {
            "p -> p.mapColor(MapColor.STONE).strength(1.5f)".into()
        } else {
            "BlockBehaviour.Properties.of().mapColor(MapColor.STONE).strength(1.5f)".into()
        },
    );
    v.insert(
        "event",
        if p.at_least("1.21.4") {
            "GatherDataEvent.Client".into()
        } else {
            "GatherDataEvent".into()
        },
    );
    v.insert(
        "client_mixin_package",
        format!("{}.mixin.client", p.package),
    );
    let main_dir = format!("src/main/java/{}", pkg_dir(&p.package));
    let render = |t: &str, extra: &[(&'static str, &str)]| {
        let mut v = v.clone();
        for (k, val) in extra {
            v.insert(k, val.to_string());
        }
        fill(t, &v, Some(&api))
    };
    out.push(text(
        format!("{main_dir}/{}.java", p.class),
        render(src::NEO_MAIN, &[]),
    ));
    out.push(text(
        format!("{main_dir}/registry/ModItems.java"),
        render(src::NEO_ITEMS, &[]),
    ));
    out.push(text(
        format!("{main_dir}/registry/ModBlocks.java"),
        render(src::NEO_BLOCKS, &[]),
    ));
    let side = if p.environment.has_client() {
        [
            ("suffix", "Client"),
            ("dist", "CLIENT"),
            ("side", "client"),
            ("setup_event", "FMLClientSetupEvent"),
        ]
    } else {
        [
            ("suffix", "Server"),
            ("dist", "DEDICATED_SERVER"),
            ("side", "dedicated server"),
            ("setup_event", "FMLDedicatedServerSetupEvent"),
        ]
    };
    out.push(text(
        format!("{main_dir}/{}{}.java", p.class, side[0].1),
        render(src::NEO_SIDE, &side),
    ));
    if p.datagen {
        out.push(text(
            format!("{main_dir}/datagen/ModDataGenerators.java"),
            render(src::NEO_DATAGEN, &[]),
        ));
    }
    if p.mixins {
        let mixin_pkg = format!("{}.mixin", p.package);
        out.push(text(
            format!("{main_dir}/mixin/ExampleMixin.java"),
            render(src::NEO_MIXIN, &[]),
        ));
        let mut json = mixin_json(p, &mixin_pkg, "mixins", &["ExampleMixin"]);
        if p.environment.has_client() {
            out.push(text(
                format!("{main_dir}/mixin/client/ExampleClientMixin.java"),
                render(src::CLIENT_MIXIN, &[]),
            ));
            json = json.replace(
                "  ],\n  \"injectors\"",
                "  ],\n  \"client\": [\n    \"client.ExampleClientMixin\"\n  ],\n  \"injectors\"",
            );
        }
        out.push(text(
            format!("src/main/resources/{}", mixin_config(p)),
            json,
        ));
    }
}

// ---- Assets -----------------------------------------------------------------------------------

fn assets(p: &ModProject, out: &mut Vec<GeneratedFile>) {
    let id = &p.mod_id;
    let base = format!("src/main/resources/assets/{id}");
    let mut lang = serde_json::Map::new();
    lang.insert(format!("item.{id}.example_item"), "Example Item".into());
    lang.insert(format!("block.{id}.example_block"), "Example Block".into());
    out.push(text(
        format!("{base}/lang/en_us.json"),
        serde_json::to_string_pretty(&lang).unwrap_or_default() + "\n",
    ));
    out.push(text(
        format!("{base}/models/item/example_item.json"),
        format!("{{\n  \"parent\": \"minecraft:item/generated\",\n  \"textures\": {{\n    \"layer0\": \"{id}:item/example_item\"\n  }}\n}}\n"),
    ));
    out.push(text(
        format!("{base}/models/block/example_block.json"),
        format!("{{\n  \"parent\": \"minecraft:block/cube_all\",\n  \"textures\": {{\n    \"all\": \"{id}:block/example_block\"\n  }}\n}}\n"),
    ));
    out.push(text(
        format!("{base}/blockstates/example_block.json"),
        format!("{{\n  \"variants\": {{\n    \"\": {{\n      \"model\": \"{id}:block/example_block\"\n    }}\n  }}\n}}\n"),
    ));
    if p.item_definitions() {
        for (item, model) in [
            ("example_item", "item/example_item"),
            ("example_block", "block/example_block"),
        ] {
            out.push(text(
                format!("{base}/items/{item}.json"),
                format!("{{\n  \"model\": {{\n    \"type\": \"minecraft:model\",\n    \"model\": \"{id}:{model}\"\n  }}\n}}\n"),
            ));
        }
    } else {
        out.push(text(
            format!("{base}/models/item/example_block.json"),
            format!("{{\n  \"parent\": \"{id}:block/example_block\"\n}}\n"),
        ));
    }
    out.push(binary(
        format!("{base}/textures/item/example_item.png"),
        png::item_texture(id),
    ));
    out.push(binary(
        format!("{base}/textures/block/example_block.png"),
        png::block_texture(id),
    ));
    if p.loader == Loader::Fabric {
        out.push(binary(format!("{base}/icon.png"), png::icon(id)));
    }
}

// ---- Other files ------------------------------------------------------------------------------

fn gitignore(p: &ModProject) -> String {
    let mut s = String::from("# Gradle\n.gradle/\nbuild/\nout/\n\n# The game directories of runClient / runServer\nrun/\nruns/\n");
    if p.loader == Loader::Neoforge && p.datagen {
        s += "\n# The data generator's cache\nsrc/generated/resources/.cache/\n";
    }
    if p.loader == Loader::Fabric && p.datagen {
        s += "\n# The data generator's cache\nsrc/main/generated/.cache/\n";
    }
    s += "\n# IDEs\n.idea/\n*.iml\n.vscode/\n.DS_Store\n";
    s
}

/// The names of the run configurations generated mods get.
pub const RUN_CLIENT: &str = "Minecraft Client";
pub const RUN_SERVER: &str = "Minecraft Server";
pub const RUN_DATA: &str = "Data Generation";

fn run_configurations(p: &ModProject) -> String {
    let mut configs = Vec::new();
    if p.environment.has_client() {
        configs.push((RUN_CLIENT, "runClient"));
    }
    configs.push((RUN_SERVER, "runServer"));
    if p.datagen {
        configs.push((
            RUN_DATA,
            match p.loader {
                Loader::Fabric => "runDatagen",
                Loader::Neoforge => "runData",
            },
        ));
    }
    let mut s = format!("selected = \"{}\"\n", configs[0].0);
    for (name, task) in configs {
        s += &format!("\n[[configuration]]\nname = \"{name}\"\ntype = \"gradle\"\n\n[configuration.options]\ntasks = \"{task}\"\n");
    }
    s
}

fn readme(p: &ModProject) -> String {
    let loader = p.loader.label();
    let mut s = format!("# {}\n\n", p.mod_name);
    if !p.description.is_empty() {
        s += &format!("{}\n\n", p.description);
    }
    let lang = if p.kotlin { "Kotlin" } else { "Java" };
    let loader_name = match p.loader {
        Loader::Fabric => "Fabric Loader",
        Loader::Neoforge => "NeoForge",
    };
    s += &format!(
        "A {loader} mod for Minecraft {} ({loader_name} {}), written in {lang} (Java {}).\n\n",
        p.mc(),
        p.loader_version,
        p.versions.java
    );
    match &p.mappings {
        Mappings::None => {
            s += "Minecraft 26.1 and newer ship without obfuscation, so the code uses Mojang's names directly.\n\n"
        }
        Mappings::Mojang => s += "The code uses Mojang's official names (Mojang mappings).\n\n",
        Mappings::Yarn(v) => s += &format!("The code uses Fabric's Yarn names ({v}).\n\n"),
        Mappings::Parchment(v) => {
            s += &format!("The code uses Mojang's names with Parchment parameter names ({v}).\n\n")
        }
    }
    s += "## Build\n\nThe project has no Gradle wrapper yet. Create it once with an installed Gradle 9 or newer:\n\n```sh\ngradle wrapper\n```\n\n";
    s += &format!(
        "Then build the mod (Gradle downloads a JDK {} with the foojay toolchain resolver when none is installed):\n\n```sh\n./gradlew build\n```\n\n",
        p.versions.java
    );
    s += &format!(
        "The mod jar is `build/libs/{}-{}.jar`. Copy it into the `mods/` folder of a Minecraft {} installation with {loader}",
        p.mod_id,
        p.mod_version,
        p.mc()
    );
    s += if p.loader == Loader::Fabric {
        " and Fabric API.\n\n"
    } else {
        ".\n\n"
    };
    s += "## Run\n\n";
    if p.environment.has_client() {
        s += &format!("- `./gradlew runClient` (the **{RUN_CLIENT}** run configuration in Forge) starts the game with the mod.\n");
    }
    s += &format!("- `./gradlew runServer` (**{RUN_SERVER}**) starts a dedicated server in `run/`. Accept the Minecraft EULA in `run/eula.txt` on the first start.\n");
    if p.datagen {
        let task = if p.loader == Loader::Fabric {
            "runDatagen"
        } else {
            "runData"
        };
        let dir = if p.loader == Loader::Fabric {
            "src/main/generated"
        } else {
            "src/generated/resources"
        };
        s += &format!(
            "- `./gradlew {task}` (**{RUN_DATA}**) runs the data generators and writes to `{dir}`.\n"
        );
    }
    s += "\n## Structure\n\n";
    let (dir, ext) = lang_dir(p);
    let pkg = pkg_dir(&p.package);
    s += &format!(
        "- `src/main/{dir}/{pkg}/{}.{ext}`: the main class\n",
        p.class
    );
    if let Some(client) = client_class(p) {
        let set = client_set(p);
        let (cdir, cext) = if p.loader == Loader::Neoforge {
            ("java", "java")
        } else {
            (dir, ext)
        };
        s += &format!(
            "- `src/{set}/{cdir}/{}.{cext}`: the client initializer\n",
            pkg_dir(&client)
        );
    } else {
        s += &format!(
            "- `src/main/{dir}/{pkg}/{}Server.{ext}`: the dedicated server initializer\n",
            p.class
        );
    }
    s += &format!(
        "- `src/main/{dir}/{pkg}/registry/`: the example item and block\n- `src/main/resources/assets/{}/`: translations, models and textures\n",
        p.mod_id
    );
    match p.loader {
        Loader::Fabric => s += "- `src/main/resources/fabric.mod.json`: the mod's metadata\n",
        Loader::Neoforge => {
            s += "- `src/main/templates/META-INF/neoforge.mods.toml`: the mod's metadata (Gradle fills in the versions)\n"
        }
    }
    if p.mixins {
        s += &format!(
            "- `src/main/resources/{}`: the mixin configuration, with the example mixins in the `mixin` package\n",
            mixin_config(p)
        );
    }
    if p.kotlin {
        s += "\nKotlin needs Fabric Language Kotlin installed next to the mod. Mixins stay Java classes.\n";
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(f: impl FnOnce(&mut ModOptions)) -> ModOptions {
        let mut o = ModOptions::default();
        f(&mut o);
        o
    }

    fn file<'a>(files: &'a [GeneratedFile], path: &str) -> &'a str {
        &files
            .iter()
            .find(|f| f.path == path)
            .unwrap_or_else(|| {
                panic!(
                    "{path} missing in {:?}",
                    files.iter().map(|f| &f.path).collect::<Vec<_>>()
                )
            })
            .contents
    }

    #[test]
    fn derives_names() {
        assert_eq!(mod_id_of("better-chests"), "better_chests");
        assert_eq!(mod_id_of("BetterChests"), "better_chests");
        assert_eq!(mod_id_of("2fast"), "mod_2fast");
        assert_eq!(mod_id_of("x"), "mymod");
        assert_eq!(display_name("better_chests"), "Better Chests");
        assert!(is_mod_id("ab"));
        assert!(!is_mod_id("a"));
        assert!(!is_mod_id("Better"));
        assert!(!is_mod_id("a-b"));
        let p = resolve("better-chests", &ModOptions::default()).unwrap();
        assert_eq!(p.package, "com.example.betterchests");
        assert_eq!(p.class, "BetterChests");
        assert_eq!(p.mod_name, "Better Chests");
        assert_eq!(p.mappings, Mappings::None);
        assert_eq!(p.loader_version, versions::FABRIC_LOADER);
    }

    #[test]
    fn validates_options() {
        let err = |f: fn(&mut ModOptions)| resolve("demo", &opts(f)).unwrap_err();
        assert!(err(|o| o.mod_id = "Demo".into()).contains("Mod ID"));
        assert!(err(|o| o.mod_id = "minecraft".into()).contains("reserved"));
        assert!(err(|o| o.group_id = "com..x".into()).contains("Group ID"));
        assert!(err(|o| o.mc_version = "1.20.1".into()).contains("1.21.1"));
        assert!(err(|o| {
            o.loader = "neoforge".into();
            o.language = "kotlin".into();
        })
        .contains("Fabric"));
        assert!(err(|o| {
            o.loader = "neoforge".into();
            o.loader_version = "21.1.265".into();
        })
        .contains("26.3"));
        assert!(err(|o| o.loader_version = "latest".into()).contains("Fabric Loader"));
        assert!(err(|o| o.mod_version = "1 0".into()).contains("mod version"));
        assert!(err(|o| o.mc_version = "27.1".into()).contains("Refresh"));
        assert!(err(|o| o.environment = "web".into()).contains("environment"));
    }

    #[test]
    fn mappings_follow_the_version() {
        let m = |f: fn(&mut ModOptions)| resolve("demo", &opts(f)).unwrap().mappings;
        assert_eq!(m(|_| {}), Mappings::None);
        assert_eq!(
            m(|o| {
                o.mc_version = "1.21.11".into();
                o.fabric_mappings = "yarn".into();
            }),
            Mappings::Yarn("1.21.11+build.6".into())
        );
        assert_eq!(m(|o| o.mc_version = "1.21.11".into()), Mappings::Mojang);
        assert_eq!(
            m(|o| {
                o.loader = "neoforge".into();
                o.mc_version = "1.21.1".into();
            }),
            Mappings::Parchment("2024.11.17".into())
        );
        assert_eq!(
            m(|o| {
                o.loader = "neoforge".into();
                o.mc_version = "1.21.1".into();
                o.neo_mappings = "official".into();
            }),
            Mappings::Mojang
        );
        // 26.x ignores the choice.
        assert_eq!(m(|o| o.fabric_mappings = "yarn".into()), Mappings::None);
    }

    #[test]
    fn fabric_26_project() {
        let files = generate_mod("better-chests", &ModOptions::default()).unwrap();
        let build = file(&files, "build.gradle");
        assert!(build.contains("id 'net.fabricmc.fabric-loom' version \"${loom_version}\""));
        assert!(build.contains("implementation \"net.fabricmc:fabric-loader:"));
        assert!(!build.contains("mappings"));
        assert!(build.contains("splitEnvironmentSourceSets()"));
        let json: serde_json::Value =
            serde_json::from_str(file(&files, "src/main/resources/fabric.mod.json")).unwrap();
        assert_eq!(json["id"], "better_chests");
        assert_eq!(
            json["entrypoints"]["main"][0],
            "com.example.betterchests.BetterChests"
        );
        assert_eq!(
            json["entrypoints"]["client"][0],
            "com.example.betterchests.client.BetterChestsClient"
        );
        assert_eq!(json["depends"]["minecraft"], "~26.3");
        assert_eq!(json["depends"]["java"], ">=25");
        assert!(files
            .iter()
            .any(|f| f.path
                == "src/client/java/com/example/betterchests/client/BetterChestsClient.java"));
        let props = file(&files, "gradle.properties");
        assert!(props.contains("fabric_api_version=0.165.2+26.3\n"));
        let items = file(
            &files,
            "src/main/java/com/example/betterchests/registry/ModItems.java",
        );
        assert!(items.contains("ResourceKey.create(Registries.ITEM, BetterChests.id(name))"));
        assert!(items.contains("properties.setId(key)"));
        let main = file(
            &files,
            "src/main/java/com/example/betterchests/BetterChests.java",
        );
        assert!(main.contains("Identifier.fromNamespaceAndPath(MOD_ID, path)"));
        let mixins: serde_json::Value =
            serde_json::from_str(file(&files, "src/main/resources/better_chests.mixins.json"))
                .unwrap();
        assert_eq!(mixins["package"], "com.example.betterchests.mixin");
        assert_eq!(mixins["compatibilityLevel"], "JAVA_25");
        let png = files
            .iter()
            .find(|f| f.path.ends_with("textures/item/example_item.png"))
            .unwrap();
        assert!(png.binary.as_ref().unwrap().starts_with(b"\x89PNG"));
        assert!(files
            .iter()
            .any(|f| f.path == "src/main/resources/assets/better_chests/items/example_block.json"));
    }

    #[test]
    fn run_configurations_are_gradle_tasks() {
        let files = generate_mod("demo", &opts(|o| o.datagen = true)).unwrap();
        let toml: toml::Value =
            toml::from_str(file(&files, ".forge/runConfigurations.toml")).unwrap();
        let cfgs = toml["configuration"].as_array().unwrap();
        let tasks: Vec<_> = cfgs
            .iter()
            .map(|c| c["options"]["tasks"].as_str().unwrap())
            .collect();
        assert_eq!(tasks, ["runClient", "runServer", "runDatagen"]);
        assert!(cfgs.iter().all(|c| c["type"].as_str() == Some("gradle")));
        assert_eq!(toml["selected"].as_str(), Some(RUN_CLIENT));
        let neo = generate_mod(
            "demo",
            &opts(|o| {
                o.loader = "neoforge".into();
                o.environment = "server".into();
                o.datagen = true;
            }),
        )
        .unwrap();
        let toml: toml::Value =
            toml::from_str(file(&neo, ".forge/runConfigurations.toml")).unwrap();
        assert_eq!(toml["selected"].as_str(), Some(RUN_SERVER));
        assert_eq!(
            toml["configuration"][1]["options"]["tasks"].as_str(),
            Some("runData")
        );
    }

    #[test]
    fn neoforge_mods_toml_is_valid_toml_after_expansion() {
        let files = generate_mod(
            "demo",
            &opts(|o| {
                o.loader = "neoforge".into();
                o.description = "Costs $5, \"quoted\"".into();
                o.authors = "Alex, Sam".into();
                o.environment = "client".into();
            }),
        )
        .unwrap();
        let raw = file(&files, "src/main/templates/META-INF/neoforge.mods.toml");
        // What Gradle's expand makes of it.
        let expanded = raw
            .replace("${mod_id}", "demo")
            .replace("${mod_version}", "1.0.0")
            .replace("${neo_version}", "26.3.0.101")
            .replace("${minecraft_version_range}", "[26.3]")
            .replace("\\$", "$");
        let toml: toml::Value = toml::from_str(&expanded).unwrap();
        assert_eq!(toml["mods"][0]["modId"].as_str(), Some("demo"));
        assert_eq!(
            toml["mods"][0]["description"].as_str(),
            Some("Costs $5, \"quoted\"")
        );
        assert_eq!(
            toml["mods"][0]["displayTest"].as_str(),
            Some("IGNORE_ALL_VERSION")
        );
        assert_eq!(
            toml["mixins"][0]["config"].as_str(),
            Some("demo.mixins.json")
        );
        assert_eq!(
            toml["dependencies"]["demo"][1]["versionRange"].as_str(),
            Some("[26.3]")
        );
        let mixins: serde_json::Value =
            serde_json::from_str(file(&files, "src/main/resources/demo.mixins.json")).unwrap();
        assert_eq!(mixins["client"][0], "client.ExampleClientMixin");
    }

    #[test]
    fn older_versions_use_their_api() {
        let yarn = generate_mod(
            "demo",
            &opts(|o| {
                o.mc_version = "1.21.4".into();
                o.fabric_mappings = "yarn".into();
            }),
        )
        .unwrap();
        let build = file(&yarn, "build.gradle");
        assert!(build.contains("id 'fabric-loom'"));
        assert!(build.contains("mappings \"net.fabricmc:yarn:${project.yarn_mappings}:v2\""));
        assert!(build.contains("modImplementation \"net.fabricmc.fabric-api"));
        let items = file(
            &yarn,
            "src/main/java/com/example/demo/registry/ModItems.java",
        );
        assert!(items.contains("RegistryKey.of(RegistryKeys.ITEM"));
        assert!(items.contains("settings") || items.contains("properties.registryKey(key)"));
        let old = generate_mod(
            "demo",
            &opts(|o| {
                o.mc_version = "1.21.1".into();
                o.loader = "neoforge".into();
                o.datagen = true;
            }),
        )
        .unwrap();
        assert!(file(&old, "build.gradle").contains("            data()\n"));
        assert!(file(&old, "build.gradle").contains("parchment {"));
        let blocks = file(
            &old,
            "src/main/java/com/example/demo/registry/ModBlocks.java",
        );
        assert!(blocks.contains("BlockBehaviour.Properties.of()"));
        assert!(old
            .iter()
            .any(|f| f.path == "src/main/resources/assets/demo/models/item/example_block.json"));
        let fabric_old = generate_mod("demo", &opts(|o| o.mc_version = "1.21.1".into())).unwrap();
        let items = file(
            &fabric_old,
            "src/main/java/com/example/demo/registry/ModItems.java",
        );
        assert!(items.contains("ResourceLocation") || !items.contains("setId"));
        assert!(!items.contains("setId"));
        assert!(
            file(&fabric_old, "src/main/java/com/example/demo/Demo.java")
                .contains("ResourceLocation.fromNamespaceAndPath")
        );
    }

    #[test]
    fn every_combination_generates() {
        for loader in ["fabric", "neoforge"] {
            for mc in versions::MOD_VERSIONS {
                for language in ["java", "kotlin"] {
                    if loader == "neoforge" && language == "kotlin" {
                        continue;
                    }
                    for env in ["both", "client", "server"] {
                        for mappings in ["a", "b"] {
                            for (mixins, datagen) in [(true, true), (false, false)] {
                                let o = opts(|o| {
                                    o.loader = loader.into();
                                    o.mc_version = mc.version.into();
                                    o.language = language.into();
                                    o.environment = env.into();
                                    o.mixins = mixins;
                                    o.datagen = datagen;
                                    o.fabric_mappings =
                                        if mappings == "a" { "mojang" } else { "yarn" }.into();
                                    o.neo_mappings = if mappings == "a" {
                                        "official"
                                    } else {
                                        "parchment"
                                    }
                                    .into();
                                });
                                let files = generate_mod("demo", &o).unwrap_or_else(|e| {
                                    panic!("{loader} {} {language} {env}: {e}", mc.version)
                                });
                                for f in &files {
                                    assert!(
                                        !f.contents.contains("{{"),
                                        "{}: placeholder in {loader} {} {language}\n{}",
                                        f.path,
                                        mc.version,
                                        f.contents
                                    );
                                    // Gradle's expand: only the properties it is given.
                                    let expanded: &[&str] = match f.path.as_str() {
                                        "src/main/resources/fabric.mod.json" => &["version"],
                                        "src/main/templates/META-INF/neoforge.mods.toml" => &[
                                            "mod_id",
                                            "mod_version",
                                            "neo_version",
                                            "minecraft_version_range",
                                        ],
                                        _ => &[],
                                    };
                                    if !expanded.is_empty() {
                                        for (i, _) in f.contents.match_indices("${") {
                                            if f.contents[..i].ends_with('\\') {
                                                continue;
                                            }
                                            let rest = &f.contents[i + 2..];
                                            let name = &rest[..rest.find('}').unwrap()];
                                            assert!(
                                                expanded.contains(&name),
                                                "{}: ${{{name}}}",
                                                f.path
                                            );
                                        }
                                    }
                                    if f.path.ends_with(".json") {
                                        let t = f.contents.replace("${version}", "1");
                                        serde_json::from_str::<serde_json::Value>(&t)
                                            .unwrap_or_else(|e| panic!("{}: {e}", f.path));
                                    }
                                }
                                let mut paths: Vec<_> = files.iter().map(|f| &f.path).collect();
                                paths.dedup();
                                assert_eq!(paths.len(), files.len());
                            }
                        }
                    }
                }
            }
        }
    }
}
