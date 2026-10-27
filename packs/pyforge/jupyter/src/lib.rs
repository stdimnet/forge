//! PyForge's Jupyter support, modelled on PyCharm's:
//!
//! - [`nbformat`]: `.ipynb` (nbformat v4) into a [`Notebook`] and back, round-trip stable.
//! - [`output`]: cell outputs and how the editor shows them ([`output::render`]).
//! - [`html`]: HTML / SVG sanitizing and HTML tables; [`dataframe`]: DataFrame tables with
//!   sorting, filtering and pages.
//! - [`diff`]: notebook diffs cell by cell (sources and outputs separately).
//! - [`protocol`]: the Jupyter messaging protocol with HMAC-SHA256 signatures.
//! - With the `runtime` feature: [`kernel`] (ZeroMQ), [`server`] (Jupyter server REST +
//!   WebSocket) and [`session`] (running cells, variables, interrupt / restart).

pub mod dataframe;
pub mod diff;
pub mod html;
pub mod nbformat;
pub mod output;
pub mod protocol;
pub mod serverurl;
pub mod variables;

#[cfg(feature = "runtime")]
pub mod kernel;
#[cfg(feature = "runtime")]
pub mod server;
#[cfg(feature = "runtime")]
pub mod session;
#[cfg(feature = "runtime")]
pub mod transport;

pub use dataframe::DataFrame;
pub use diff::{diff_notebooks, diff_texts, NotebookDiff};
pub use nbformat::{new_notebook, parse, serialize, Cell, CellType, Notebook, NotebookError};
pub use output::{render, Output, Rendered};
pub use variables::Variable;

use serde::{Deserialize, Serialize};

/// A notebook as the editor opens it: the model and the rendered outputs of every cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookView {
    pub notebook: Notebook,
    /// `rendered[cell][output]`.
    pub rendered: Vec<Vec<Rendered>>,
}

/// Read a notebook and render its outputs (`root`: the project folder, for file links in
/// tracebacks).
pub fn open(text: &str, root: Option<&str>) -> Result<NotebookView, NotebookError> {
    let notebook = parse(text)?;
    let rendered = render_cells(&notebook, root);
    Ok(NotebookView { notebook, rendered })
}

/// Rendered outputs of every cell.
pub fn render_cells(nb: &Notebook, root: Option<&str>) -> Vec<Vec<Rendered>> {
    nb.cells
        .iter()
        .map(|c| c.outputs.iter().map(|o| render(o, root)).collect())
        .collect()
}

/// Render a list of outputs.
pub fn render_outputs(outputs: &[Output], root: Option<&str>) -> Vec<Rendered> {
    outputs.iter().map(|o| render(o, root)).collect()
}
