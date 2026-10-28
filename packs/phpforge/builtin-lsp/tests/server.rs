//! End-to-end: the hidden `--php-lsp --stdio` mode, started and driven by forge-lsp's client
//! with the configuration Forge builds from its PHP settings, on a small project. No network.

use std::path::{Path, PathBuf};
use std::time::Duration;

use forge_lsp::lsp_types::{
    CompletionResponse, Diagnostic, DocumentChanges, FormattingOptions, GotoDefinitionResponse, HoverContents,
    MarkedString, Position,
};
use forge_lsp::{Feature, LspClient, LspEvent, LspServerConfig};
use phpforge_lang_php::lsp::{php_lsp_config, BuiltinServer, PhpLspSettings};
use tokio::sync::broadcast;

const USER: &str = "<?php

namespace App\\Models;

class User
{
    public function __construct(private string $name)
    {
    }

    /** The user's display name. */
    public function displayName(): string
    {
        return $this->name;
    }
}
";

const SERVICE: &str = "<?php

namespace App;

use App\\Models\\User;

class Greeter
{
    public function greet(User $user): string
    {
        return 'Hello ' . $user->displayName();
    }
}
";

fn write(root: &Path, rel: &str, text: &str) -> PathBuf {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    path
}

/// The config Forge registers for the built-in server, with this test's host binary.
fn config(cache: &Path) -> LspServerConfig {
    let server = BuiltinServer {
        command: env!("CARGO_BIN_EXE_phpforge-builtin-lsp-host").into(),
        args: phpforge_builtin_lsp::server_args(),
        version: phpforge_builtin_lsp::version_label(),
    };
    let settings = PhpLspSettings {
        php_version: "8.3".into(),
        storage_path: Some(cache.to_path_buf()),
        ..Default::default()
    };
    let cfg = php_lsp_config(&server, &settings, Some(r#"{"require": {"php": "^8.3"}}"#));
    LspServerConfig {
        id: "php".into(),
        name: Some(cfg.name),
        language_ids: cfg.language_ids,
        file_extensions: cfg.file_extensions,
        command: cfg.command.to_string_lossy().into_owned(),
        args: cfg.args,
        initialization_options: cfg.initialization_options,
        settings: cfg.settings,
        ..Default::default()
    }
}

/// Retry `f` until it yields a value: cross-file answers wait for background indexing.
async fn eventually<T, F, Fut>(mut f: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Some(v) = f().await {
                return v;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("no answer from php-lsp in time")
}

async fn diagnostics_for(
    rx: &mut broadcast::Receiver<LspEvent>,
    file: &Path,
    want: impl Fn(&[Diagnostic]) -> bool,
) -> Vec<Diagnostic> {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match rx.recv().await {
                Ok(LspEvent::Diagnostics {
                    uri, diagnostics, ..
                }) if uri.to_file_path().ok().as_deref() == Some(file) && want(&diagnostics) => {
                    return diagnostics
                }
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(e) => panic!("event channel: {e}"),
            }
        }
    })
    .await
    .expect("expected diagnostics did not arrive")
}

fn hover_text(contents: HoverContents) -> String {
    match contents {
        HoverContents::Markup(m) => m.value,
        HoverContents::Scalar(MarkedString::String(s)) => s,
        HoverContents::Scalar(MarkedString::LanguageString(s)) => s.value,
        HoverContents::Array(a) => a
            .into_iter()
            .map(|m| match m {
                MarkedString::String(s) => s,
                MarkedString::LanguageString(s) => s.value,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

#[tokio::test]
async fn the_hidden_mode_serves_a_php_project() {
    let project = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let root = project.path().canonicalize().unwrap();
    write(&root, "composer.json", r#"{"autoload": {"psr-4": {"App\\": "src/"}}}"#);
    let user = write(&root, "src/Models/User.php", USER);
    let service = write(&root, "src/Greeter.php", SERVICE);

    let (tx, mut rx) = broadcast::channel(1024);
    let client = LspClient::start_with_events(config(cache.path()), &root, tx)
        .await
        .expect("php-lsp starts in the hidden mode");

    // initialize: the features Forge wires up are announced.
    for feature in [
        Feature::Hover,
        Feature::Definition,
        Feature::References,
        Feature::Implementation,
        Feature::TypeDefinition,
        Feature::Rename,
        Feature::CodeAction,
        Feature::SignatureHelp,
        Feature::TypeHierarchy,
    ] {
        assert!(client.supports(feature), "{feature:?}");
    }
    assert!(client.supports_formatting());
    assert!(client.supports_prepare_rename());
    assert!(client.supports_inlay_hints());

    client.did_open(&service, SERVICE).unwrap();

    // hover on `displayName` in Greeter (line 10).
    let call = Position::new(10, SERVICE.lines().nth(10).unwrap().find("displayName").unwrap() as u32 + 2);
    let hover = eventually(|| async {
        client
            .hover(&service, call)
            .await
            .ok()
            .flatten()
            .map(|h| hover_text(h.contents))
            .filter(|t| t.contains("displayName"))
    })
    .await;
    assert!(hover.contains("display name"), "{hover}");

    // definition: across files, into User.php.
    let target = eventually(|| async {
        let found = client.definition(&service, call).await.ok().flatten()?;
        let loc = match found {
            GotoDefinitionResponse::Scalar(l) => l,
            GotoDefinitionResponse::Array(mut a) => a.pop()?,
            GotoDefinitionResponse::Link(mut a) => {
                let l = a.pop()?;
                forge_lsp::lsp_types::Location::new(l.target_uri, l.target_selection_range)
            }
        };
        Some(loc)
    })
    .await;
    assert_eq!(target.uri.to_file_path().unwrap(), user);
    assert_eq!(target.range.start.line, 11);

    // completion after `$user->`.
    let typed = SERVICE.replace("$user->displayName();", "$user->");
    client.did_change(&service, &typed).unwrap();
    let at = Position::new(10, typed.lines().nth(10).unwrap().len() as u32);
    let labels: Vec<String> = eventually(|| async {
        let labels: Vec<String> = match client.completion(&service, at).await.ok().flatten()? {
            CompletionResponse::Array(items) => items.into_iter().map(|i| i.label).collect(),
            CompletionResponse::List(list) => list.items.into_iter().map(|i| i.label).collect(),
        };
        labels
            .iter()
            .any(|l| l.starts_with("displayName"))
            .then_some(labels)
    })
    .await;
    assert!(labels.len() <= 2, "{labels:?}");

    // diagnostics: an undefined function.
    let broken = SERVICE.replace("return 'Hello '", "undefined_helper();\n        return 'Hello '");
    client.did_change(&service, &broken).unwrap();
    let diags = diagnostics_for(&mut rx, &service, |d| {
        d.iter().any(|d| d.message.contains("undefined_helper"))
    })
    .await;
    assert!(diags.iter().all(|d| d.source.as_deref() != Some("phpstan")));
    client.did_change(&service, SERVICE).unwrap();

    // formatting: the built-in formatter in Intelephense's style.
    let ugly = "<?php\nfunction f($a){if($a){return 1;}return 2;}\n";
    let scratch = write(&root, "src/scratch.php", ugly);
    client.did_open(&scratch, ugly).unwrap();
    let edits = client
        .formatting(
            &scratch,
            FormattingOptions {
                tab_size: 4,
                insert_spaces: true,
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .expect("formatting edits");
    assert!(!edits.is_empty());
    assert!(edits.iter().any(|e| e.new_text.contains('\n')), "{edits:?}");

    // rename: the method and its call in the other file.
    let decl = Position::new(11, USER.lines().nth(11).unwrap().find("displayName").unwrap() as u32 + 1);
    client.did_open(&user, USER).unwrap();
    assert!(client.prepare_rename(&user, decl).await.unwrap().is_some());
    let edit = eventually(|| async {
        let edit = client.rename(&user, decl, "label").await.ok().flatten()?;
        let mut files: Vec<PathBuf> = Vec::new();
        if let Some(changes) = &edit.changes {
            files.extend(changes.keys().filter_map(|u| u.to_file_path().ok()));
        }
        if let Some(DocumentChanges::Edits(docs)) = &edit.document_changes {
            files.extend(docs.iter().filter_map(|d| d.text_document.uri.to_file_path().ok()));
        }
        (files.contains(&user) && files.contains(&service)).then_some(files)
    })
    .await;
    assert_eq!(edit.len(), 2, "{edit:?}");

    client.shutdown().await;
}
