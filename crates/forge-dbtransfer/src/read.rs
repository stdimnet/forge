//! Import sources: CSV / TSV (RFC 4180 with the usual quirks: a byte order mark, CRLF,
//! quoted line breaks, doubled quotes, ragged rows, a sniffed delimiter and legacy
//! encodings), Excel workbooks (`.xlsx`, `.xls`, `.ods`, read with calamine) and JSON (an
//! array of objects or arrays, or newline-delimited JSON).

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceFormat {
    #[default]
    Csv,
    Tsv,
    Excel,
    Json,
}

impl SourceFormat {
    /// The format of a file name (`.tsv`, `.xlsx`, `.json`, `.ndjson`, else CSV).
    pub fn from_path(path: &str) -> Self {
        let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        match ext.as_str() {
            "tsv" | "tab" => SourceFormat::Tsv,
            "xlsx" | "xlsm" | "xls" | "xlsb" | "ods" => SourceFormat::Excel,
            "json" | "ndjson" | "jsonl" => SourceFormat::Json,
            _ => SourceFormat::Csv,
        }
    }
}

/// The options of the import dialog's source side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReadOptions {
    pub format: SourceFormat,
    /// Field separator; `None` sniffs it (CSV) or uses a tab (TSV).
    pub delimiter: Option<char>,
    pub quote: char,
    /// The first row names the columns.
    pub header: bool,
    /// `utf-8`, `utf-16le`, `windows-1252`, … (any WHATWG label); empty detects a BOM and
    /// falls back to Windows-1252 for bytes that are not UTF-8.
    pub encoding: String,
    /// Excel: the sheet to read (empty: the first).
    pub sheet: String,
    /// Lines (rows) to skip before the header.
    pub skip_rows: usize,
    pub trim: bool,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            format: SourceFormat::Csv,
            delimiter: None,
            quote: '"',
            header: true,
            encoding: String::new(),
            sheet: String::new(),
            skip_rows: 0,
            trim: false,
        }
    }
}

/// Rows read from a source. `None` cells are empty (unquoted empty CSV fields, JSON
/// `null`, empty Excel cells); `Some("")` is an empty string (`""` in CSV).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SourceTable {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    /// Excel: every sheet of the workbook.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sheets: Vec<String>,
    /// CSV: the delimiter used (the sniffed one when none was given).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<char>,
    /// The encoding the text was decoded with.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub encoding: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Read a whole source.
pub fn read(bytes: &[u8], opts: &ReadOptions) -> Result<SourceTable, String> {
    match opts.format {
        SourceFormat::Excel => read_excel(bytes, opts),
        SourceFormat::Json => {
            let (text, encoding) = decode(bytes, &opts.encoding)?;
            let mut t = read_json(&text)?;
            t.encoding = encoding;
            Ok(t)
        }
        SourceFormat::Csv | SourceFormat::Tsv => {
            let (text, encoding) = decode(bytes, &opts.encoding)?;
            let delimiter = match (opts.delimiter, opts.format) {
                (Some(d), _) => d,
                (None, SourceFormat::Tsv) => '\t',
                _ => sniff_delimiter(&text, opts.quote),
            };
            let (records, mut warnings) = parse_csv(&text, delimiter, opts.quote);
            let mut t = tabulate(records, opts, &mut warnings);
            t.delimiter = Some(delimiter);
            t.encoding = encoding;
            t.warnings = warnings;
            Ok(t)
        }
    }
}

/// Text of `bytes` in `label`'s encoding, or detected: a BOM, valid UTF-8, else
/// Windows-1252.
pub fn decode(bytes: &[u8], label: &str) -> Result<(String, String), String> {
    let label = label.trim();
    if !label.is_empty() {
        let enc = encoding_rs::Encoding::for_label(label.as_bytes())
            .ok_or_else(|| format!("unknown encoding {label}"))?;
        let (text, _, _) = enc.decode(bytes);
        return Ok((text.into_owned(), enc.name().to_string()));
    }
    if let Some((enc, _)) = encoding_rs::Encoding::for_bom(bytes) {
        let (text, _, _) = enc.decode(bytes);
        return Ok((text.into_owned(), enc.name().to_string()));
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => Ok((s.to_string(), "UTF-8".into())),
        Err(_) => {
            let (text, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
            Ok((text.into_owned(), "windows-1252".into()))
        }
    }
}

/// The delimiter that splits the first lines into the same number of fields (most fields
/// wins): `,`, `;`, tab or `|`.
pub fn sniff_delimiter(text: &str, quote: char) -> char {
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(10)
        .collect();
    let mut best = (',', 0usize, false);
    for cand in [',', ';', '\t', '|'] {
        let counts: Vec<usize> = lines
            .iter()
            .map(|l| {
                let mut quoted = false;
                l.chars()
                    .filter(|c| {
                        if *c == quote {
                            quoted = !quoted;
                        }
                        *c == cand && !quoted
                    })
                    .count()
            })
            .collect();
        let first = counts.first().copied().unwrap_or(0);
        if first == 0 {
            continue;
        }
        let consistent = counts.iter().all(|c| *c == first);
        if (consistent, first) > (best.2, best.1) {
            best = (cand, first, consistent);
        }
    }
    best.0
}

/// Records of CSV text. Empty unquoted fields are `None`.
pub fn parse_csv(
    text: &str,
    delimiter: char,
    quote: char,
) -> (Vec<Vec<Option<String>>>, Vec<String>) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut records = Vec::new();
    let mut warnings = Vec::new();
    let mut record: Vec<Option<String>> = Vec::new();
    let mut field = String::new();
    let mut quoted = false; // inside quotes
    let mut was_quoted = false; // the field started with a quote
    let mut chars = text.chars().peekable();
    let mut line = 1usize;
    let mut quote_line = 0usize;
    let end_field =
        |record: &mut Vec<Option<String>>, field: &mut String, was_quoted: &mut bool| {
            let f = std::mem::take(field);
            record.push(if f.is_empty() && !*was_quoted {
                None
            } else {
                Some(f)
            });
            *was_quoted = false;
        };
    while let Some(c) = chars.next() {
        if quoted {
            if c == quote {
                if chars.peek() == Some(&quote) {
                    chars.next();
                    field.push(quote);
                } else {
                    quoted = false;
                }
            } else {
                if c == '\n' {
                    line += 1;
                }
                field.push(c);
            }
            continue;
        }
        match c {
            c if c == quote && field.is_empty() && !was_quoted => {
                quoted = true;
                was_quoted = true;
                quote_line = line;
            }
            c if c == delimiter => end_field(&mut record, &mut field, &mut was_quoted),
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                line += 1;
                end_field(&mut record, &mut field, &mut was_quoted);
                // Blank lines are skipped.
                if !(record.len() == 1 && record[0].is_none()) {
                    records.push(std::mem::take(&mut record));
                } else {
                    record.clear();
                }
            }
            // A quote inside an unquoted field (or after a closing quote) is literal.
            c => field.push(c),
        }
    }
    if quoted {
        warnings.push(format!(
            "A quoted field that starts on line {quote_line} is not closed; it runs to the end of the file"
        ));
    }
    if !field.is_empty() || was_quoted || !record.is_empty() {
        end_field(&mut record, &mut field, &mut was_quoted);
        if !(record.len() == 1 && record[0].is_none()) {
            records.push(record);
        }
    }
    (records, warnings)
}

/// Unique column names: blanks become `column_N`, duplicates get `_2`, `_3`.
fn column_names(raw: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, n) in raw.into_iter().enumerate() {
        let base = if n.trim().is_empty() {
            format!("column_{}", i + 1)
        } else {
            n.trim().to_string()
        };
        let mut name = base.clone();
        let mut k = 2;
        while out.iter().any(|o| o.eq_ignore_ascii_case(&name)) {
            name = format!("{base}_{k}");
            k += 1;
        }
        out.push(name);
    }
    out
}

/// Header, skipped rows, trimming and ragged rows of raw records.
fn tabulate(
    mut records: Vec<Vec<Option<String>>>,
    opts: &ReadOptions,
    warnings: &mut Vec<String>,
) -> SourceTable {
    records.drain(..opts.skip_rows.min(records.len()));
    if opts.trim {
        for r in &mut records {
            for c in r.iter_mut().flatten() {
                *c = c.trim().to_string();
            }
        }
    }
    let width = records.iter().map(Vec::len).max().unwrap_or(0);
    let header: Vec<String> = if opts.header && !records.is_empty() {
        records
            .remove(0)
            .into_iter()
            .map(|c| c.unwrap_or_default())
            .collect()
    } else {
        Vec::new()
    };
    let mut raw = header;
    while raw.len() < width {
        raw.push(String::new());
    }
    let columns = column_names(raw);
    let ragged = records.iter().filter(|r| r.len() != columns.len()).count();
    if ragged > 0 {
        warnings.push(format!(
            "{ragged} row(s) have a different number of fields; missing ones are empty"
        ));
    }
    for r in &mut records {
        r.resize(columns.len(), None);
    }
    SourceTable {
        columns,
        rows: records,
        ..Default::default()
    }
}

fn json_cell(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        other => Some(other.to_string()),
    }
}

/// An array of objects (columns: every key, in first-seen order) or arrays (the first is
/// the header when it holds only strings), or one JSON object per line.
pub fn read_json(text: &str) -> Result<SourceTable, String> {
    let trimmed = text.trim_start_matches('\u{feff}').trim();
    let items: Vec<Value> = if trimmed.starts_with('[') {
        match serde_json::from_str::<Value>(trimmed) {
            Ok(Value::Array(a)) => a,
            Ok(_) => return Err("expected a JSON array".into()),
            Err(e) => return Err(format!("invalid JSON: {e}")),
        }
    } else {
        let mut out = Vec::new();
        for (i, l) in trimmed.lines().enumerate() {
            let l = l.trim();
            if l.is_empty() {
                continue;
            }
            out.push(
                serde_json::from_str::<Value>(l)
                    .map_err(|e| format!("line {}: invalid JSON: {e}", i + 1))?,
            );
        }
        out
    };
    let mut t = SourceTable::default();
    if items.iter().all(Value::is_array) && !items.is_empty() {
        let rows: Vec<Vec<Option<String>>> = items
            .iter()
            .map(|v| {
                v.as_array()
                    .map(|a| a.iter().map(json_cell).collect())
                    .unwrap_or_default()
            })
            .collect();
        let header = items[0]
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string));
        let mut warnings = Vec::new();
        return Ok(tabulate(
            rows,
            &ReadOptions {
                header,
                ..Default::default()
            },
            &mut warnings,
        ));
    }
    for v in &items {
        let Value::Object(m) = v else {
            return Err("expected JSON objects (one row each)".into());
        };
        for k in m.keys() {
            if !t.columns.contains(k) {
                t.columns.push(k.clone());
            }
        }
    }
    t.rows = items
        .iter()
        .map(|v| {
            t.columns
                .iter()
                .map(|c| v.get(c).and_then(json_cell))
                .collect()
        })
        .collect();
    Ok(t)
}

#[cfg(feature = "xlsx")]
fn read_excel(bytes: &[u8], opts: &ReadOptions) -> Result<SourceTable, String> {
    use calamine::{open_workbook_auto_from_rs, Data, Reader};
    let mut wb = open_workbook_auto_from_rs(std::io::Cursor::new(bytes.to_vec()))
        .map_err(|e| format!("cannot read the workbook: {e}"))?;
    let sheets = wb.sheet_names().to_vec();
    let sheet = if opts.sheet.is_empty() {
        sheets
            .first()
            .cloned()
            .ok_or("the workbook has no sheets")?
    } else {
        opts.sheet.clone()
    };
    let range = wb
        .worksheet_range(&sheet)
        .map_err(|e| format!("cannot read sheet {sheet}: {e}"))?;
    let mut errors = 0;
    let records: Vec<Vec<Option<String>>> = range
        .rows()
        .map(|row| {
            row.iter()
                .map(|c| match c {
                    Data::Empty => None,
                    Data::String(s) => Some(s.clone()),
                    Data::Int(i) => Some(i.to_string()),
                    Data::Float(f) => Some(if f.fract() == 0.0 && f.abs() < 1e15 {
                        format!("{}", *f as i64)
                    } else {
                        f.to_string()
                    }),
                    Data::Bool(b) => Some(b.to_string()),
                    Data::DateTime(dt) => Some(match dt.as_datetime() {
                        Some(t) if t.time() == chrono_midnight() => {
                            t.format("%Y-%m-%d").to_string()
                        }
                        Some(t) => t.format("%Y-%m-%d %H:%M:%S").to_string(),
                        None => dt.as_f64().to_string(),
                    }),
                    Data::DateTimeIso(s) | Data::DurationIso(s) => Some(s.clone()),
                    Data::Error(_) => {
                        errors += 1;
                        None
                    }
                })
                .collect()
        })
        .collect();
    let mut warnings = Vec::new();
    if errors > 0 {
        warnings.push(format!(
            "{errors} cell(s) hold Excel errors; they are empty"
        ));
    }
    let mut t = tabulate(records, opts, &mut warnings);
    t.sheets = sheets;
    t.warnings = warnings;
    Ok(t)
}

#[cfg(feature = "xlsx")]
fn chrono_midnight() -> chrono::NaiveTime {
    chrono::NaiveTime::MIN
}

#[cfg(not(feature = "xlsx"))]
fn read_excel(_bytes: &[u8], _opts: &ReadOptions) -> Result<SourceTable, String> {
    Err("Excel files are read by the desktop app".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn csv(text: &str) -> SourceTable {
        read(text.as_bytes(), &ReadOptions::default()).unwrap()
    }

    #[test]
    fn csv_quirks() {
        let t = csv("\u{feff}id,name,note\r\n1,\"Doe, Jane\",\"said \"\"hi\"\"\"\r\n2,,\"\"\r\n\r\n3,\"multi\nline\",x\"y\n");
        assert_eq!(t.columns, ["id", "name", "note"]);
        assert_eq!(t.rows.len(), 3);
        assert_eq!(
            t.rows[0],
            [
                Some("1".into()),
                Some("Doe, Jane".into()),
                Some("said \"hi\"".into())
            ]
        );
        // Empty unquoted is NULL, "" is an empty string.
        assert_eq!(t.rows[1], [Some("2".into()), None, Some(String::new())]);
        assert_eq!(t.rows[2][1].as_deref(), Some("multi\nline"));
        assert_eq!(t.rows[2][2].as_deref(), Some("x\"y"));
        assert_eq!(t.delimiter, Some(','));
        assert_eq!(t.encoding, "UTF-8");
    }

    #[test]
    fn sniffs_delimiters_and_handles_ragged_rows() {
        let t = csv("a;b;c\n1;2;3\n4;5\n6;7;8;9\n");
        assert_eq!(t.delimiter, Some(';'));
        assert_eq!(t.columns, ["a", "b", "c", "column_4"]);
        assert_eq!(t.rows[1], [Some("4".into()), Some("5".into()), None, None]);
        assert!(t.warnings[0].contains("2 row(s)"));
        assert_eq!(sniff_delimiter("x\ty\n1\t2\n", '"'), '\t');
        assert_eq!(sniff_delimiter("\"a,b\"|c\n1|2\n", '"'), '|');
    }

    #[test]
    fn header_options_and_duplicate_names() {
        let opts = ReadOptions {
            header: false,
            skip_rows: 1,
            trim: true,
            ..Default::default()
        };
        let t = read(b"# export\n 1 , a \n2,b", &opts).unwrap();
        assert_eq!(t.columns, ["column_1", "column_2"]);
        assert_eq!(t.rows[0], [Some("1".into()), Some("a".into())]);
        let t = csv("id,Name,name,\n1,2,3,4\n");
        assert_eq!(t.columns, ["id", "Name", "name_2", "column_4"]);
    }

    #[test]
    fn unterminated_quote_and_custom_quote() {
        let t = csv("a,b\n1,\"open\n2,3\n");
        assert_eq!(t.rows.len(), 1);
        assert!(t.warnings[0].contains("line 2"));
        let opts = ReadOptions {
            quote: '\'',
            delimiter: Some('|'),
            ..Default::default()
        };
        let t = read(b"a|b\n'x|y'|z", &opts).unwrap();
        assert_eq!(t.rows[0][0].as_deref(), Some("x|y"));
    }

    #[test]
    fn decodes_legacy_encodings() {
        // "Müller" in Windows-1252 and UTF-16LE with a BOM.
        let t = read(b"name\nM\xfcller\n", &ReadOptions::default()).unwrap();
        assert_eq!(t.rows[0][0].as_deref(), Some("Müller"));
        assert_eq!(t.encoding, "windows-1252");
        let mut utf16 = vec![0xff, 0xfe];
        for u in "n\nü\n".encode_utf16() {
            utf16.extend(u.to_le_bytes());
        }
        let t = read(&utf16, &ReadOptions::default()).unwrap();
        assert_eq!(t.rows[0][0].as_deref(), Some("ü"));
        assert_eq!(t.encoding, "UTF-16LE");
        let opts = ReadOptions {
            encoding: "latin1".into(),
            ..Default::default()
        };
        assert_eq!(
            read(b"n\n\xe9\n", &opts).unwrap().rows[0][0].as_deref(),
            Some("é")
        );
        assert!(read(
            b"n",
            &ReadOptions {
                encoding: "klingon".into(),
                ..Default::default()
            }
        )
        .is_err());
    }

    #[test]
    fn tsv_and_formats() {
        let opts = ReadOptions {
            format: SourceFormat::Tsv,
            ..Default::default()
        };
        let t = read(b"a\tb\n1,5\t2\n", &opts).unwrap();
        assert_eq!(t.rows[0][0].as_deref(), Some("1,5"));
        assert_eq!(SourceFormat::from_path("x/Data.XLSX"), SourceFormat::Excel);
        assert_eq!(SourceFormat::from_path("rows.ndjson"), SourceFormat::Json);
        assert_eq!(SourceFormat::from_path("rows.txt"), SourceFormat::Csv);
    }

    #[test]
    fn json_arrays_and_ndjson() {
        let t = read_json(r#"[{"id":1,"name":"a","tags":["x"]},{"id":2,"extra":null,"ok":true}]"#)
            .unwrap();
        assert_eq!(t.columns, ["id", "name", "tags", "extra", "ok"]);
        assert_eq!(t.rows[0][2].as_deref(), Some("[\"x\"]"));
        assert_eq!(
            t.rows[1],
            [Some("2".into()), None, None, None, Some("true".into())]
        );
        let t = read_json("{\"a\":1}\n\n{\"a\":2,\"b\":\"x\"}\n").unwrap();
        assert_eq!(t.columns, ["a", "b"]);
        assert_eq!(t.rows.len(), 2);
        let t = read_json(r#"[["id","name"],[1,"a"],[2,null]]"#).unwrap();
        assert_eq!(t.columns, ["id", "name"]);
        assert_eq!(t.rows[1][1], None);
        assert!(read_json("{\"a\":1}\nnot json")
            .unwrap_err()
            .contains("line 2"));
        assert!(read_json("[1, 2]").is_err());
    }
}
