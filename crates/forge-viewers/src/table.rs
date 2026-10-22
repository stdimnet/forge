//! CSV/TSV for "Edit as Table". The parser remembers which cells were quoted, so writing an
//! unchanged table gives back the same text, and edits only touch the cells they change.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

/// A parsed CSV/TSV file. Rows may differ in length (missing cells are empty).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvTable {
    pub rows: Vec<Vec<String>>,
    /// Per cell: written in quotes.
    pub quoted: Vec<Vec<bool>>,
    pub delimiter: char,
    /// `\n` or `\r\n`.
    pub eol: String,
    /// The text ends with a line break.
    pub trailing_newline: bool,
    /// The widest row.
    pub columns: usize,
}

/// One change of the table editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum CsvEdit {
    SetCell {
        row: usize,
        column: usize,
        value: String,
    },
    /// An empty row before `at` (at the end when `at` is past the last row).
    InsertRow {
        at: usize,
    },
    RemoveRows {
        rows: Vec<usize>,
    },
    /// An empty column before `at`.
    InsertColumn {
        at: usize,
    },
    RemoveColumns {
        columns: Vec<usize>,
    },
}

/// The delimiter of a file: tab for `.tsv`/`.tab`, else the most frequent of `,` `;` tab
/// and `|` in the first line (outside quotes), comma by default.
pub fn detect_delimiter(path: &str, text: &str) -> char {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".tsv") || lower.ends_with(".tab") {
        return '\t';
    }
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let mut counts = [(',', 0usize), (';', 0), ('\t', 0), ('|', 0)];
    let mut quoted = false;
    for c in first.chars() {
        if c == '"' {
            quoted = !quoted;
        } else if !quoted {
            if let Some(e) = counts.iter_mut().find(|(d, _)| *d == c) {
                e.1 += 1;
            }
        }
    }
    counts
        .iter()
        .filter(|(_, n)| *n > 0)
        .max_by_key(|(_, n)| *n)
        .map_or(',', |(d, _)| *d)
}

/// Parse CSV/TSV text (RFC 4180 quoting; a quote inside an unquoted cell is literal).
pub fn parse_table(text: &str, delimiter: char) -> CsvTable {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut table = CsvTable {
        delimiter,
        eol: eol.into(),
        trailing_newline: text.ends_with('\n'),
        ..CsvTable::default()
    };
    if text.is_empty() {
        return table;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut row: Vec<String> = Vec::new();
    let mut row_quoted: Vec<bool> = Vec::new();
    loop {
        // One cell.
        let mut cell = String::new();
        let mut quoted = false;
        if chars.get(i) == Some(&'"') {
            quoted = true;
            i += 1;
            while i < chars.len() {
                if chars[i] == '"' {
                    if chars.get(i + 1) == Some(&'"') {
                        cell.push('"');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                cell.push(chars[i]);
                i += 1;
            }
        }
        while i < chars.len() && chars[i] != delimiter && chars[i] != '\n' {
            if chars[i] == '\r' && chars.get(i + 1) == Some(&'\n') {
                i += 1;
                continue;
            }
            cell.push(chars[i]);
            i += 1;
        }
        row.push(cell);
        row_quoted.push(quoted);
        match chars.get(i) {
            Some(c) if *c == delimiter => i += 1,
            Some(_) => {
                // End of line.
                i += 1;
                table.rows.push(std::mem::take(&mut row));
                table.quoted.push(std::mem::take(&mut row_quoted));
                if i >= chars.len() {
                    break;
                }
            }
            None => {
                table.rows.push(std::mem::take(&mut row));
                table.quoted.push(std::mem::take(&mut row_quoted));
                break;
            }
        }
    }
    table.columns = table.rows.iter().map(Vec::len).max().unwrap_or(0);
    table
}

fn needs_quotes(cell: &str, delimiter: char) -> bool {
    cell.contains(delimiter) || cell.contains(['"', '\n', '\r'])
}

impl CsvTable {
    /// The table as text, quoting cells that were quoted or need it.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (r, row) in self.rows.iter().enumerate() {
            if r > 0 {
                out.push_str(&self.eol);
            }
            for (c, cell) in row.iter().enumerate() {
                if c > 0 {
                    out.push(self.delimiter);
                }
                let quoted = self
                    .quoted
                    .get(r)
                    .and_then(|q| q.get(c))
                    .copied()
                    .unwrap_or(false);
                if quoted || needs_quotes(cell, self.delimiter) {
                    out.push('"');
                    out.push_str(&cell.replace('"', "\"\""));
                    out.push('"');
                } else {
                    out.push_str(cell);
                }
            }
        }
        if self.trailing_newline && !self.rows.is_empty() {
            out.push_str(&self.eol);
        }
        out
    }

    fn apply(&mut self, edit: CsvEdit) {
        let width = self.columns.max(1);
        match edit {
            CsvEdit::SetCell { row, column, value } => {
                while self.rows.len() <= row {
                    self.rows.push(vec![String::new(); width]);
                    self.quoted.push(vec![false; width]);
                }
                let cells = &mut self.rows[row];
                let quoted = &mut self.quoted[row];
                if cells.len() <= column {
                    cells.resize(column + 1, String::new());
                }
                quoted.resize(cells.len(), false);
                cells[column] = value;
            }
            CsvEdit::InsertRow { at } => {
                let at = at.min(self.rows.len());
                self.rows.insert(at, vec![String::new(); width]);
                self.quoted.insert(at, vec![false; width]);
            }
            CsvEdit::RemoveRows { mut rows } => {
                rows.sort_unstable();
                rows.dedup();
                for r in rows.into_iter().rev() {
                    if r < self.rows.len() {
                        self.rows.remove(r);
                        self.quoted.remove(r);
                    }
                }
            }
            CsvEdit::InsertColumn { at } => {
                for (cells, quoted) in self.rows.iter_mut().zip(self.quoted.iter_mut()) {
                    // Short rows are padded so the new column lines up.
                    if cells.len() < at {
                        cells.resize(at, String::new());
                        quoted.resize(at, false);
                    }
                    cells.insert(at, String::new());
                    quoted.insert(at, false);
                }
            }
            CsvEdit::RemoveColumns { mut columns } => {
                columns.sort_unstable();
                columns.dedup();
                for (cells, quoted) in self.rows.iter_mut().zip(self.quoted.iter_mut()) {
                    for &c in columns.iter().rev() {
                        if c < cells.len() {
                            cells.remove(c);
                            quoted.remove(c);
                        }
                    }
                    if cells.is_empty() {
                        cells.push(String::new());
                        quoted.push(false);
                    }
                }
            }
        }
        self.columns = self.rows.iter().map(Vec::len).max().unwrap_or(0);
    }
}

/// Apply edits to CSV/TSV text and return the new text.
pub fn apply_edits(text: &str, delimiter: char, edits: Vec<CsvEdit>) -> String {
    let mut table = parse_table(text, delimiter);
    if table.rows.is_empty() {
        table.trailing_newline = true;
    }
    for e in edits {
        table.apply(e);
    }
    table.to_text()
}

fn compare_cells(a: &str, b: &str) -> Ordering {
    let (ta, tb) = (a.trim(), b.trim());
    // Empty cells last, numbers before text, numbers by value, text case-insensitively.
    match (ta.is_empty(), tb.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    match (ta.parse::<f64>(), tb.parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        (Ok(_), Err(_)) => Ordering::Less,
        (Err(_), Ok(_)) => Ordering::Greater,
        _ => ta
            .to_lowercase()
            .cmp(&tb.to_lowercase())
            .then_with(|| ta.cmp(tb)),
    }
}

/// Display order of the data rows (`skip` header rows stay first) sorted by `column`;
/// the file itself is not changed.
pub fn sort_order(
    rows: &[Vec<String>],
    column: usize,
    descending: bool,
    skip: usize,
) -> Vec<usize> {
    let skip = skip.min(rows.len());
    let mut order: Vec<usize> = (skip..rows.len()).collect();
    let cell = |r: usize| rows[r].get(column).map_or("", String::as_str);
    order.sort_by(|&a, &b| {
        let o = compare_cells(cell(a), cell(b));
        // Empty cells stay last in both directions.
        let empty = cell(a).trim().is_empty() || cell(b).trim().is_empty();
        if descending && !empty {
            o.reverse()
        } else {
            o
        }
    });
    (0..skip).chain(order).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_quoting_delimiter_and_final_newline() {
        for text in [
            "id,name,note\n1,\"Pen\",\"says \"\"hi\"\"\"\n2,Ink,\"a, b\"\n",
            "a;b;c\n1;2;3",
            "x\ty\n\"multi\nline\"\tz\n",
            "a,b\r\n1,2\r\n",
            "one\n\ntwo\n",
            "short,row\nlonger,row,here\n",
        ] {
            let d = detect_delimiter("f.csv", text);
            let table = parse_table(text, d);
            assert_eq!(table.to_text(), text, "{text:?}");
        }
    }

    #[test]
    fn parses_cells_and_quotes() {
        let t = parse_table("id,name\n1,\"Pen, blue\"\n2,Ink\n", ',');
        assert_eq!(
            t.rows,
            [vec!["id", "name"], vec!["1", "Pen, blue"], vec!["2", "Ink"]]
        );
        assert_eq!(t.quoted[1], [false, true]);
        assert_eq!(t.columns, 2);
        assert!(t.trailing_newline);
        let t = parse_table("a\t\"b\"\"c\"\n", '\t');
        assert_eq!(t.rows[0], ["a", "b\"c"]);
        assert!(parse_table("", ',').rows.is_empty());
    }

    #[test]
    fn detects_delimiters() {
        assert_eq!(detect_delimiter("data.tsv", "a,b"), '\t');
        assert_eq!(detect_delimiter("data.csv", "a;b;\"c,d\"\n"), ';');
        assert_eq!(detect_delimiter("data.csv", "a|b|c"), '|');
        assert_eq!(detect_delimiter("data.csv", "single"), ',');
    }

    #[test]
    fn edits_keep_untouched_cells_as_written() {
        let text = "id,name\n1,\"Pen\"\n2,Ink\n";
        let out = apply_edits(
            text,
            ',',
            vec![CsvEdit::SetCell {
                row: 2,
                column: 1,
                value: "Ink, black".into(),
            }],
        );
        assert_eq!(out, "id,name\n1,\"Pen\"\n2,\"Ink, black\"\n");
        let out = apply_edits(
            text,
            ',',
            vec![
                CsvEdit::InsertRow { at: 1 },
                CsvEdit::InsertColumn { at: 1 },
            ],
        );
        assert_eq!(out, "id,,name\n,,\n1,,\"Pen\"\n2,,Ink\n");
        let out = apply_edits(
            text,
            ',',
            vec![
                CsvEdit::RemoveRows { rows: vec![1] },
                CsvEdit::RemoveColumns { columns: vec![0] },
            ],
        );
        assert_eq!(out, "name\nInk\n");
        let out = apply_edits(
            "a,b\n",
            ',',
            vec![CsvEdit::SetCell {
                row: 1,
                column: 3,
                value: "x".into(),
            }],
        );
        assert_eq!(out, "a,b\n,,,x\n");
        assert_eq!(
            apply_edits(
                "",
                ',',
                vec![CsvEdit::SetCell {
                    row: 0,
                    column: 0,
                    value: "v".into()
                }]
            ),
            "v\n"
        );
    }

    #[test]
    fn sorts_numbers_text_and_keeps_header_and_empty_cells() {
        let rows: Vec<Vec<String>> = [["qty"], ["10"], ["9"], [""], ["apple"], ["Banana"]]
            .iter()
            .map(|r| r.iter().map(|s| s.to_string()).collect())
            .collect();
        assert_eq!(sort_order(&rows, 0, false, 1), [0, 2, 1, 4, 5, 3]);
        assert_eq!(sort_order(&rows, 0, true, 1), [0, 5, 4, 1, 2, 3]);
        assert_eq!(sort_order(&rows, 3, false, 0), [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn edits_deserialize_from_the_ui() {
        let e: Vec<CsvEdit> = serde_json::from_str(r#"[{"op":"setCell","row":1,"column":0,"value":"x"},{"op":"removeColumns","columns":[2]}]"#).unwrap();
        assert_eq!(e.len(), 2);
    }
}
