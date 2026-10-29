//! A smoke test against the real Eclipse JDT LS and java-debug: both are downloaded from
//! their official sources (checksums checked) into a temporary folder, JDT LS starts with a
//! JDK of this machine it can run on (else a Temurin download) the way JavaForge starts it, completes a `String` and opens a
//! java-debug DAP server. Needs network access and a JDK, so it only runs on request:
//!
//! ```sh
//! cargo test -p javaforge-jdk --test jdtls_smoke -- --ignored --nocapture
//! ```

use std::time::{Duration, Instant};

use forge_langservers::managed::{javadebug, jdtls, Http};
use forge_lsp::lsp_types::{CompletionResponse, Position};
use forge_lsp::{LspClient, LspServerConfig};
use javaforge_jdk::adoptium::{self, Platform};
use javaforge_jdk::{detect, java_executable, pick, DetectEnv};
use javaforge_lang_java::jdtls::{config_folder, JdtlsSetup, Runtime};

#[tokio::test]
#[ignore = "downloads Eclipse JDT LS and java-debug; needs network and a JDK 21+"]
async fn jdtls_completes_and_starts_a_debug_session() {
    let tmp = tempfile::tempdir().unwrap();
    let http = Http::new().unwrap();
    let install = jdtls::install(
        &http,
        jdtls::DEFAULT_MIRROR,
        jdtls::Channel::Milestones,
        &tmp.path().join("jdtls"),
        &mut |e| eprintln!("{e:?}"),
    )
    .await
    .expect("JDT LS installs");
    // The JDK it needs (21, 25 for newer builds): this machine's, else a Temurin download.
    let jdks = detect(&DetectEnv::system());
    let jdk = match pick(&jdks, None, install.java).cloned() {
        Some(j) => j,
        None => {
            let platform = Platform::current().expect("a Temurin platform");
            adoptium::download(
                &http,
                adoptium::DEFAULT_API,
                install.java,
                &platform,
                &tmp.path().join("jdks"),
                &mut |_| {},
            )
            .await
            .expect("Temurin downloads")
        }
    };
    eprintln!(
        "JDT LS {} on Java {} ({})",
        install.version, jdk.major, jdk.home
    );
    let debug_jar = javadebug::install(
        &http,
        javadebug::DEFAULT_REPOSITORY,
        &tmp.path().join("java-debug"),
        &mut |_| {},
    )
    .await
    .expect("java-debug installs");

    let project = tmp.path().join("hello");
    std::fs::create_dir_all(&project).unwrap();
    let source = "public class Hello {\n    public static void main(String[] args) {\n        String s = \"forge\";\n        s.\n    }\n}\n";
    let file = project.join("Hello.java");
    std::fs::write(&file, source).unwrap();

    let setup = JdtlsSetup {
        java: java_executable(&jdk.home, cfg!(windows)),
        launcher_jar: install.launcher_jar.display().to_string(),
        shared_config: install
            .dir
            .join(config_folder(std::env::consts::OS, std::env::consts::ARCH))
            .display()
            .to_string(),
        config_dir: tmp.path().join("cache/config").display().to_string(),
        data_dir: tmp.path().join("cache/workspace").display().to_string(),
        bundles: vec![debug_jar.display().to_string()],
        runtimes: vec![Runtime {
            major: jdk.major,
            path: jdk.home.clone(),
            default: true,
        }],
        use_wrapper: true,
        hot_code_replace: "auto".into(),
        ..Default::default()
    };
    let config = LspServerConfig {
        id: "jdtls".into(),
        name: Some("Eclipse JDT LS".into()),
        language_ids: vec!["java".into()],
        file_extensions: vec!["java".into()],
        command: setup.java.clone(),
        args: setup.args(),
        initialization_options: setup.initialization_options(),
        settings: setup.settings(),
        ..Default::default()
    };
    let client = LspClient::start(config, &project)
        .await
        .expect("JDT LS starts");
    client.did_open(&file, source).unwrap();

    let deadline = Instant::now() + Duration::from_secs(180);
    let labels = loop {
        let answer = client
            .completion(
                &file,
                Position {
                    line: 3,
                    character: 10,
                },
            )
            .await
            .ok()
            .flatten();
        let labels: Vec<String> = match answer {
            Some(CompletionResponse::Array(items)) => items.into_iter().map(|i| i.label).collect(),
            Some(CompletionResponse::List(list)) => {
                list.items.into_iter().map(|i| i.label).collect()
            }
            None => Vec::new(),
        };
        if labels.iter().any(|l| l.starts_with("length")) || Instant::now() > deadline {
            break labels;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    };
    assert!(
        labels.iter().any(|l| l.starts_with("length")),
        "completions: {labels:?}"
    );

    let port = client
        .execute_command("vscode.java.startDebugSession", vec![])
        .await
        .expect("java-debug answers")
        .and_then(|v| v.as_u64())
        .expect("a DAP port");
    assert!(port > 0);
    client.shutdown().await;
}
