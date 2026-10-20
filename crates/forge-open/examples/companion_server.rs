//! The Browser Companion API without the IDE, for developing the browser extension:
//! prints a pairing code and every open request it receives.
//!
//! ```sh
//! cargo run -p forge-open --example companion_server -- /path/to/project [port]
//! ```

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use forge_open::pairing::{now_secs, ClientInfo, Pairing};
use forge_open::server::{
    CompanionServer, HostError, HostInfo, OpenHost, OpenOutcome, OpenStatus, ProjectInfo,
    DEFAULT_PORT,
};
use forge_open::{
    open_event, KnownProject, OpenRequest, OpenSource, ProjectContext, SymbolRequest,
};

struct PrintingHost {
    root: PathBuf,
}

impl PrintingHost {
    fn context(&self) -> ProjectContext {
        ProjectContext {
            open: Some(KnownProject::new("project", &self.root)),
            recent: vec![],
        }
    }
}

impl OpenHost for PrintingHost {
    fn info(&self) -> HostInfo {
        HostInfo {
            name: "Forge".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            projects: vec![ProjectInfo {
                name: "project".into(),
                root: self.root.display().to_string(),
            }],
        }
    }

    fn open(&self, request: OpenRequest, client: &ClientInfo) -> Result<OpenOutcome, HostError> {
        let event = open_event(
            &request,
            &self.context(),
            OpenSource::Browser,
            Some(client.name.clone()),
        )
        .map_err(|e| HostError::new(404, e.to_string()))?;
        println!("open {}", serde_json::to_string(&event).unwrap_or_default());
        Ok(OpenOutcome {
            status: if event.opens_directly() {
                OpenStatus::Opened
            } else {
                OpenStatus::NeedsConfirmation
            },
            file: event.file,
            line: event.line,
        })
    }

    fn open_symbol(
        &self,
        request: SymbolRequest,
        _: &ClientInfo,
    ) -> Result<OpenOutcome, HostError> {
        let ws = forge_core::Workspace::open(&self.root)
            .map_err(|e| HostError::new(500, e.to_string()))?;
        let hit = forge_open::symbols::resolve_symbol(&ws, &request)
            .map_err(|e| HostError::new(500, e.to_string()))?
            .into_iter()
            .next()
            .ok_or_else(|| HostError::new(404, format!("no declaration of {}", request.name)))?;
        println!(
            "open-symbol {} -> {}:{:?}",
            request.name, hit.path, hit.line
        );
        Ok(OpenOutcome {
            status: OpenStatus::Opened,
            file: self.root.join(&hit.path).display().to_string(),
            line: hit.line,
        })
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().unwrap_or_else(|| ".".into())).canonicalize()?;
    let port = args
        .next()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let pairing = Arc::new(Mutex::new(Pairing::in_memory()));
    let code = pairing.lock().expect("pairing lock").start(now_secs());
    let server = CompanionServer::start(port, Arc::new(PrintingHost { root }), pairing).await?;
    println!("listening on 127.0.0.1:{}", server.port);
    println!("pairing code {}", code.code);
    tokio::signal::ctrl_c().await
}
