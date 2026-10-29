//! A real debug session with js-debug and Node.js: a breakpoint in a TypeScript file whose
//! compiled JavaScript has a source map. Ignored by default (needs Node.js and an unpacked
//! js-debug):
//!
//! ```sh
//! JS_DEBUG_DIR=/path/to/js-debug cargo test -p webforge-js-debug -- --ignored
//! ```

use std::sync::Arc;
use std::time::Duration;

use forge_dap::{DapManager, LaunchRequest};
use forge_debug::{BreakpointSpec, DebugEvent, DebugServer, EventSink, SessionStatus};
use tokio::sync::mpsc;
use webforge_js_debug::{JsDebugAdapter, JsDebugConfig};

/// `src/greet.ts`, compiled by tsc to `dist/greet.js` with a source map.
const TS: &str = "function greet(name: string): string {\n  const message: string = `Hello, ${name}`;\n  return message;\n}\n\nconsole.log(greet(\"Forge\"));\n";
const JS: &str = "function greet(name) {
    const message = `Hello, ${name}`;
    return message;
}
console.log(greet(\"Forge\"));
//# sourceMappingURL=greet.js.map
";
const MAP: &str = r#"{"version":3,"file":"greet.js","sourceRoot":"","sources":["../src/greet.ts"],"names":[],"mappings":"AAAA,SAAS,KAAK,CAAC,IAAY;IACzB,MAAM,OAAO,GAAW,UAAU,IAAI,EAAE,CAAC;IACzC,OAAO,OAAO,CAAC;AACjB,CAAC;AAED,OAAO,CAAC,GAAG,CAAC,KAAK,CAAC,OAAO,CAAC,CAAC,CAAC"}"#;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Node.js and js-debug (JS_DEBUG_DIR)"]
async fn typescript_breakpoint_through_a_source_map() {
    let dir = std::env::var("JS_DEBUG_DIR").expect("JS_DEBUG_DIR");
    let server = webforge_js_debug::release::user_server(std::path::Path::new(&dir))
        .expect("dapDebugServer.js in JS_DEBUG_DIR");
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src")).unwrap();
    std::fs::create_dir_all(root.path().join("dist")).unwrap();
    std::fs::write(root.path().join("src/greet.ts"), TS).unwrap();
    std::fs::write(root.path().join("dist/greet.js"), JS).unwrap();
    std::fs::write(root.path().join("dist/greet.js.map"), MAP).unwrap();

    let (tx, mut events) = mpsc::unbounded_channel();
    let sink: EventSink = Arc::new(move |e: DebugEvent| {
        let _ = tx.send(e);
    });
    let debug = Arc::new(DebugServer::new(Arc::clone(&sink)));
    debug.open_project(Some(root.path()));
    let manager = Arc::new(DapManager::new(sink, debug.clone()));
    manager.set_project_root(Some(root.path().to_path_buf()));
    for a in JsDebugAdapter::all(&JsDebugConfig {
        node: "node".into(),
        server: Some(server),
        skip_files: vec!["<node_internals>/**".into()],
    }) {
        manager.register_adapter(Arc::new(a));
    }
    debug
        .add_breakpoint(BreakpointSpec {
            file: Some("src/greet.ts".into()),
            line: Some(3),
            ..BreakpointSpec::default()
        })
        .unwrap();
    let request = LaunchRequest::launch("pwa-node", "dist/greet.js");
    manager.start(request).await.unwrap();
    let paused = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            match events.recv().await {
                Some(DebugEvent::Session { session })
                    if session.status == SessionStatus::Paused =>
                {
                    return session
                }
                Some(_) => continue,
                None => panic!("events ended"),
            }
        }
    })
    .await
    .expect("paused at the breakpoint");
    assert!(
        paused.parent_id.is_some(),
        "the debuggee runs in js-debug's child session"
    );
    let at = paused.location.expect("a location");
    assert_eq!(at.file.as_deref(), Some("src/greet.ts"), "{at:?}");
    assert_eq!(at.line, 3);
    manager.stop_all().await;
}
