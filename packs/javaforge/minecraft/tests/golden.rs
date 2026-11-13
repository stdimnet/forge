//! Golden files of the Minecraft Plugin generator: every generated file of a few typical
//! projects, concatenated per case in `tests/golden/<case>.txt`. Regenerate them with
//! `FORGE_UPDATE_SNAPSHOTS=1 cargo test -p javaforge-minecraft --test golden`.

use javaforge_minecraft::mods::{generate_mod, ModOptions};
use javaforge_minecraft::{generate, GeneratedFile, Options};

fn render(files: Vec<GeneratedFile>) -> String {
    let mut out = String::new();
    for f in files {
        if let Some(bytes) = &f.binary {
            // Binary files (textures): their size and a checksum.
            let sum = bytes
                .iter()
                .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(*b as u32));
            out += &format!(
                "==== {} ====\n(binary, {} bytes, checksum {sum:08x})\n",
                f.path,
                bytes.len()
            );
            continue;
        }
        out += &format!("==== {} ====\n{}", f.path, f.contents);
        if !f.contents.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

fn check(case: &str, name: &str, options: Options) {
    compare(case, render(generate(name, &options).unwrap()));
}

fn check_mod(case: &str, name: &str, options: ModOptions) {
    compare(case, render(generate_mod(name, &options).unwrap()));
}

fn compare(case: &str, actual: String) {
    let path = format!("{}/tests/golden/{case}.txt", env!("CARGO_MANIFEST_DIR"));
    if std::env::var_os("FORGE_UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("{path} is missing; run with FORGE_UPDATE_SNAPSHOTS=1"))
        .replace("\r\n", "\n");
    assert!(
        actual == expected,
        "{case} differs from {path}; run with FORGE_UPDATE_SNAPSHOTS=1 and review the diff\n{actual}"
    );
}

fn o(f: impl FnOnce(&mut Options)) -> Options {
    let mut o = Options::default();
    f(&mut o);
    o
}

#[test]
fn paper_gradle_kotlin_dsl_java() {
    check(
        "paper-gradle-kts-java",
        "better-chat",
        o(|o| {
            o.authors = "Alex, Sam".into();
            o.description = "Nicer chat messages".into();
            o.website = "https://example.com".into();
        }),
    );
}

#[test]
fn paper_plugin_yml_kotlin_with_shadow() {
    check(
        "paper-plugin-yml-kotlin",
        "better-chat",
        o(|o| {
            o.descriptor = "paper-plugin".into();
            o.language = "kotlin".into();
            o.group_id = "dev.alex".into();
        }),
    );
}

#[test]
fn spigot_maven_java() {
    check(
        "spigot-maven",
        "better-chat",
        o(|o| {
            o.platform = "spigot".into();
            o.build_system = "maven".into();
            o.mc_version = "1.20.4".into();
            o.shadow = true;
        }),
    );
}

#[test]
fn purpur_gradle_groovy() {
    check(
        "purpur-gradle-groovy",
        "better-chat",
        o(|o| {
            o.platform = "purpur".into();
            o.build_system = "gradle-groovy".into();
            o.mc_version = "1.21.4".into();
        }),
    );
}

#[test]
fn velocity_gradle_kotlin_dsl() {
    check(
        "velocity-gradle-kts",
        "lobby-proxy",
        o(|o| {
            o.platform = "velocity".into();
            o.authors = "Alex".into();
        }),
    );
}

#[test]
fn bungeecord_gradle_kotlin() {
    check(
        "bungeecord-kotlin",
        "lobby-proxy",
        o(|o| {
            o.platform = "bungeecord".into();
            o.language = "kotlin".into();
        }),
    );
}

#[test]
fn paper_26_maven_with_a_pinned_build() {
    check(
        "paper-26-maven",
        "better-chat",
        o(|o| {
            o.build_system = "maven".into();
            o.mc_version = "26.3".into();
        }),
    );
}

#[test]
fn purpur_26_gradle_with_a_dynamic_build() {
    check(
        "purpur-26-gradle-kts",
        "better-chat",
        o(|o| {
            o.platform = "purpur".into();
            o.mc_version = "26.2".into();
        }),
    );
}

fn m(f: impl FnOnce(&mut ModOptions)) -> ModOptions {
    let mut o = ModOptions::default();
    f(&mut o);
    o
}

#[test]
fn fabric_mod_26_java() {
    check_mod(
        "mod-fabric-26-java",
        "better-chests",
        m(|o| {
            o.authors = "Alex, Sam".into();
            o.description = "Bigger chests".into();
            o.license = "MIT".into();
        }),
    );
}

#[test]
fn fabric_mod_kotlin_with_datagen() {
    check_mod(
        "mod-fabric-26-kotlin-datagen",
        "better-chests",
        m(|o| {
            o.language = "kotlin".into();
            o.datagen = true;
        }),
    );
}

#[test]
fn fabric_mod_1_21_yarn_server() {
    check_mod(
        "mod-fabric-1.21.11-yarn-server",
        "better-chests",
        m(|o| {
            o.mc_version = "1.21.11".into();
            o.fabric_mappings = "yarn".into();
            o.environment = "server".into();
            o.mixins = false;
        }),
    );
}

#[test]
fn neoforge_mod_26_java() {
    check_mod(
        "mod-neoforge-26-java",
        "better-chests",
        m(|o| {
            o.loader = "neoforge".into();
            o.authors = "Alex".into();
            o.description = "Bigger chests".into();
        }),
    );
}

#[test]
fn neoforge_mod_1_21_1_parchment_datagen() {
    check_mod(
        "mod-neoforge-1.21.1-parchment-datagen",
        "better-chests",
        m(|o| {
            o.loader = "neoforge".into();
            o.mc_version = "1.21.1".into();
            o.datagen = true;
            o.environment = "client".into();
        }),
    );
}

/// Writes generated mods for a manual Gradle build: `FORGE_MC_MOD_EXPORT=<dir>`.
#[test]
fn export_mods_for_a_smoke_build() {
    let Some(dir) = std::env::var_os("FORGE_MC_MOD_EXPORT") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let cases: Vec<(&str, ModOptions)> = vec![
        ("fabric-java", m(|o| o.datagen = true)),
        (
            "neoforge-java",
            m(|o| {
                o.loader = "neoforge".into();
                o.datagen = true;
            }),
        ),
        ("fabric-kotlin", m(|o| o.language = "kotlin".into())),
        (
            "fabric-1.21.11-yarn",
            m(|o| {
                o.mc_version = "1.21.11".into();
                o.fabric_mappings = "yarn".into();
            }),
        ),
        (
            "neoforge-1.21.1",
            m(|o| {
                o.loader = "neoforge".into();
                o.mc_version = "1.21.1".into();
                o.datagen = true;
            }),
        ),
    ];
    for (name, o) in cases {
        for f in generate_mod(name, &o).unwrap() {
            let p = dir.join(name).join(&f.path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            match &f.binary {
                Some(b) => std::fs::write(&p, b).unwrap(),
                None => std::fs::write(&p, &f.contents).unwrap(),
            }
        }
    }
}
