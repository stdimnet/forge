//! Fixture notebooks: written by Python's `json.dumps` exactly as Jupyter writes them
//! (`sales*.ipynb`) and by an older tool (`legacy.ipynb`: nbformat 4.2, no ids, string
//! sources, two-space indent, `\u` escapes).

use pyforge_jupyter::diff::CellStatus;
use pyforge_jupyter::{diff_texts, open, parse, serialize, CellType, Output, Rendered};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn fixtures_round_trip_byte_for_byte() {
    for name in ["sales.ipynb", "sales-v2.ipynb", "legacy.ipynb"] {
        let text = fixture(name);
        let nb = parse(&text).unwrap();
        assert_eq!(serialize(&nb, Some(&text)), text, "{name}");
    }
}

#[test]
fn edited_legacy_notebook_keeps_its_style() {
    let text = fixture("legacy.ipynb");
    let mut nb = parse(&text).unwrap();
    assert!(!nb.has_ids);
    assert_eq!(nb.cells[2].cell_type, CellType::Raw);
    nb.cells[0].source = "Café report, Zürich".into();
    nb.cells.swap(1, 2);
    let out = serialize(&nb, Some(&text));
    assert!(
        out.contains("\"source\": \"Caf\\u00e9 report, Z\\u00fcrich\""),
        "{out}"
    );
    assert!(
        out.contains("\n      \"metadata\": {\n        \"collapsed\": true\n      },"),
        "{out}"
    );
    assert!(!out.contains("\"id\""));
    assert!(!out.ends_with('\n'));
    let back = parse(&out).unwrap();
    assert_eq!(back.cells[2].execution_count, Some(7));
    assert_eq!(back.cells[1].cell_type, CellType::Raw);
}

#[test]
fn renders_the_sales_outputs() {
    let view = open(&fixture("sales.ipynb"), Some("/work/shop")).unwrap();
    assert_eq!(view.notebook.cells.len(), 7);
    match &view.rendered[2][0] {
        Rendered::DataFrame { frame } => {
            assert_eq!(
                frame.columns,
                ["id", "region", "orders", "revenue", "avg_basket"]
            );
            assert_eq!(frame.rows.len(), 5);
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(&view.rendered[4][0], Rendered::Image { mime, .. } if mime == "image/png"));
    match &view.rendered[6][0] {
        Rendered::Text { lines, error } => {
            assert!(*error);
            assert!(lines
                .last()
                .unwrap()
                .text()
                .starts_with("KeyError: 'profit'"));
            assert!(lines
                .iter()
                .any(|l| l.segments.iter().any(|s| s.style.fg.is_some())));
        }
        other => panic!("{other:?}"),
    }
    assert!(
        matches!(&view.notebook.cells[1].outputs[0], Output::Stream { name, .. } if name == "stdout")
    );
}

#[test]
fn diffs_the_sales_notebooks() {
    let d = diff_texts(&fixture("sales.ipynb"), &fixture("sales-v2.ipynb")).unwrap();
    let statuses: Vec<(CellStatus, Option<&str>)> = d
        .cells
        .iter()
        .map(|c| {
            (
                c.status,
                c.right.as_ref().or(c.left.as_ref()).map(|s| s.id.as_str()),
            )
        })
        .collect();
    assert_eq!(
        statuses,
        [
            (CellStatus::Modified, Some("intro")),
            (CellStatus::Modified, Some("imports")),
            (CellStatus::Unchanged, Some("table")),
            (CellStatus::Unchanged, Some("plot-md")),
            (CellStatus::Unchanged, Some("plot")),
            (CellStatus::Modified, Some("totals")),
            (CellStatus::Removed, Some("error")),
            (CellStatus::Added, Some("share")),
        ]
    );
    let imports = &d.cells[1];
    assert!(imports.source.changed && imports.outputs.changed);
    assert_eq!((imports.source.inserted, imports.source.deleted), (1, 1));
    assert!(!d.cells[0].outputs.changed);
    assert_eq!(d.stats.unchanged, 3);
}
