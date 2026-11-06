//! Opening files in Forge from the outside world:
//!
//! - [`request`]: open requests and `forge://open?file=…&line=…` links,
//! - [`cli`]: the `forge path/to/file:12` launcher,
//! - [`launch`]: where a launch opens (project window, LightEdit window, a folder as project),
//! - [`resolve`]: which project a file belongs to (open, recent, or none),
//! - [`symbols`]: find a component/class declaration by name,
//! - [`server`] + [`pairing`]: the Browser Companion's local HTTP API,
//! - [`live_edit`]: Live Edit events the extension applies to open pages,
//! - [`project`]: project detection and dotenv edits for framework integrations.

pub mod cli;
pub mod http;
pub mod launch;
pub mod live_edit;
pub mod pairing;
pub mod project;
pub mod remote;
pub mod request;
pub mod resolve;
pub mod server;
pub mod symbols;

use serde::Serialize;

pub use remote::{resolve_remote, RemoteProject};
pub use request::{LinkAction, OpenRequest, RequestError, SymbolKind, SymbolRequest};
pub use resolve::{resolve_file, KnownProject, ProjectContext, ResolveError, Target};

/// Where an open request came from. Only [`OpenSource::Cli`] is trusted to switch projects
/// or open files outside known projects without asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OpenSource {
    Browser,
    Link,
    Cli,
}

/// Event sent to the UI to show a file (`companion://open`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenEvent {
    pub source: OpenSource,
    /// Name of the paired browser for [`OpenSource::Browser`].
    pub client: Option<String>,
    /// Absolute, canonical path.
    pub file: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub target: Target,
}

impl OpenEvent {
    /// Whether the UI shows the file without asking.
    pub fn opens_directly(&self) -> bool {
        matches!(self.target, Target::Open { .. }) || self.source == OpenSource::Cli
    }
}

/// Resolve a request into the event the UI handles. When the request has no line but names
/// a symbol, the symbol's declaration in a known project gives the position.
pub fn open_event(
    request: &OpenRequest,
    ctx: &ProjectContext,
    source: OpenSource,
    client: Option<String>,
) -> Result<OpenEvent, ResolveError> {
    let (abs, target) = resolve_file(request, ctx)?;
    let (mut line, mut column) = (request.line, request.column);
    if line.is_none() && !matches!(target, Target::Outside { .. }) {
        if let Some(symbol) = &request.symbol {
            if let Some((l, c)) = std::fs::read_to_string(&abs)
                .ok()
                .and_then(|content| symbols::find_declaration(&content, symbol))
            {
                line = Some(l);
                column = Some(c);
            }
        }
    }
    Ok(OpenEvent {
        source,
        client,
        file: abs.display().to_string(),
        line,
        column,
        target,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_gives_the_position_when_line_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/Cart.tsx"),
            "import x from 'y';\nexport function Cart() {}\n",
        )
        .unwrap();
        let ctx = ProjectContext {
            open: Some(KnownProject::new("shop", &root)),
            recent: vec![],
        };
        let request = OpenRequest {
            symbol: Some("Cart".into()),
            ..OpenRequest::new("src/Cart.tsx")
        };
        let event = open_event(&request, &ctx, OpenSource::Browser, None).unwrap();
        assert_eq!((event.line, event.column), (Some(2), Some(17)));
        assert!(event.opens_directly());

        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["target"]["kind"], "open");
        assert_eq!(json["target"]["path"], "src/Cart.tsx");
        assert_eq!(json["source"], "browser");
    }
}
