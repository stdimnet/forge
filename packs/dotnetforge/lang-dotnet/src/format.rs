//! Code cleanup with `dotnet format`: its JSON report (`--report`) as the list of changed
//! files and fixes, and the EditorConfig files that apply to a source file (Reformat Code
//! on an unsaved editor copies them next to the copy it formats, so the same rules apply
//! without touching the project).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::paths::{normalize, parent, relative_to};

/// One fix `dotnet format` made (or would make with `--verify-no-changes`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatChange {
    /// Relative to the project root when inside it.
    pub path: String,
    /// 0-based.
    pub line: u32,
    pub column: u32,
    /// `WHITESPACE`, `IDE0055`, `CA1822`.
    pub diagnostic: String,
    pub description: String,
}

/// `format-report.json`.
pub fn parse_format_report(json: &str, root: &str) -> Result<Vec<FormatChange>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("format report: {e}"))?;
    let docs = v.as_array().ok_or("the format report is not a list")?;
    let mut out = Vec::new();
    for d in docs {
        let path = d.get("FilePath").and_then(Value::as_str).unwrap_or("");
        let rel = relative_to(root, path).unwrap_or_else(|| normalize(path));
        for c in d
            .get("FileChanges")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            out.push(FormatChange {
                path: rel.clone(),
                line: c
                    .get("LineNumber")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    .saturating_sub(1) as u32,
                column: c
                    .get("CharNumber")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    .saturating_sub(1) as u32,
                diagnostic: c
                    .get("DiagnosticId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                description: c
                    .get("FormatDescription")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            });
        }
    }
    Ok(out)
}

/// The `.editorconfig` candidates of a file, from the project root down to its folder
/// (the order EditorConfig applies them; a `root = true` file stops the search upwards).
pub fn editorconfig_candidates(file: &str) -> Vec<String> {
    let mut dirs = Vec::new();
    let mut d = parent(&normalize(file));
    loop {
        dirs.push(d.clone());
        if d.is_empty() {
            break;
        }
        d = parent(&d);
    }
    dirs.reverse();
    dirs.into_iter()
        .map(|d| {
            if d.is_empty() {
                ".editorconfig".to_string()
            } else {
                format!("{d}/.editorconfig")
            }
        })
        .collect()
}

/// Whether an `.editorconfig` declares `root = true`.
pub fn is_root_editorconfig(text: &str) -> bool {
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            return false;
        }
        if let Some((k, v)) = l.split_once('=') {
            if k.trim().eq_ignore_ascii_case("root") && v.trim().eq_ignore_ascii_case("true") {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_and_editorconfig() {
        let json = r#"[{"DocumentId":{"ProjectId":{"Id":"1"},"Id":"2"},"FileName":"Program.cs","FilePath":"/p/Bookstore/src/Bookstore.Api/Program.cs","FileChanges":[{"LineNumber":12,"CharNumber":5,"DiagnosticId":"WHITESPACE","FormatDescription":"Fix whitespace formatting. Insert '\\s'."},{"LineNumber":3,"CharNumber":1,"DiagnosticId":"IMPORTS","FormatDescription":"Fix imports ordering."}],"ProjectName":"Bookstore.Api"}]"#;
        let c = parse_format_report(json, "/p/Bookstore").unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].path, "src/Bookstore.Api/Program.cs");
        assert_eq!((c[0].line, c[0].column), (11, 4));
        assert_eq!(c[1].diagnostic, "IMPORTS");
        assert!(parse_format_report("{}", "/p").is_err());
        assert_eq!(parse_format_report("[]", "/p").unwrap().len(), 0);
        assert_eq!(
            editorconfig_candidates("src/App/Program.cs"),
            vec![
                ".editorconfig",
                "src/.editorconfig",
                "src/App/.editorconfig"
            ]
        );
        assert!(is_root_editorconfig("# x\nroot = true\n[*]\n"));
        assert!(!is_root_editorconfig("[*]\nroot = true\n"));
    }
}
