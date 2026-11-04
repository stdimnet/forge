//! Golden files of the Minecraft Plugin generator: every generated file of a few typical
//! projects, concatenated per case in `tests/golden/<case>.txt`. Regenerate them with
//! `FORGE_UPDATE_SNAPSHOTS=1 cargo test -p javaforge-minecraft --test golden`.

use javaforge_minecraft::{generate, Options};

fn render(name: &str, options: &Options) -> String {
    let mut out = String::new();
    for f in generate(name, options).unwrap() {
        out += &format!("==== {} ====\n{}", f.path, f.contents);
        if !f.contents.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

fn check(case: &str, name: &str, options: Options) {
    let path = format!("{}/tests/golden/{case}.txt", env!("CARGO_MANIFEST_DIR"));
    let actual = render(name, &options);
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
