//! "Export to Excel": a single-sheet `.xlsx` workbook (Office Open XML in a ZIP with stored
//! entries), with a bold header row, numbers and booleans as such and text as inline
//! strings. Small enough to run in the browser build too.

use serde_json::Value;

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *slot = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for b in data {
        crc = table[((crc ^ *b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

/// A ZIP archive of stored (uncompressed) entries.
fn zip(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    // 1980-01-01 00:00 in DOS format.
    let (time, date) = (0u16, (1u16 << 5) | 1);
    for (name, data) in entries {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let size = data.len() as u32;
        let header = |sig: u32, central: bool| {
            let mut h = Vec::new();
            h.extend(sig.to_le_bytes());
            if central {
                h.extend(20u16.to_le_bytes()); // version made by
            }
            h.extend(20u16.to_le_bytes()); // version needed
            h.extend(0x0800u16.to_le_bytes()); // UTF-8 names
            h.extend(0u16.to_le_bytes()); // stored
            h.extend(time.to_le_bytes());
            h.extend(date.to_le_bytes());
            h.extend(crc.to_le_bytes());
            h.extend(size.to_le_bytes());
            h.extend(size.to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes()); // extra
            if central {
                h.extend(0u16.to_le_bytes()); // comment
                h.extend(0u16.to_le_bytes()); // disk
                h.extend(0u16.to_le_bytes()); // internal attributes
                h.extend(0u32.to_le_bytes()); // external attributes
                h.extend(offset.to_le_bytes());
            }
            h.extend(name.as_bytes());
            h
        };
        out.extend(header(0x0403_4b50, false));
        out.extend(data);
        central.extend(header(0x0201_4b50, true));
    }
    let cd_offset = out.len() as u32;
    let cd_size = central.len() as u32;
    out.extend(central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend(cd_size.to_le_bytes());
    out.extend(cd_offset.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Control characters are not allowed in XML 1.0.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// `A`, `B`, …, `Z`, `AA`, …
fn column_letter(mut i: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'A' + (i % 26) as u8);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

fn cell(r: usize, c: usize, v: &Value, style: u8) -> String {
    let at = format!("{}{}", column_letter(c), r + 1);
    let s = if style > 0 {
        format!(" s=\"{style}\"")
    } else {
        String::new()
    };
    let text = |t: &str| {
        format!(
            "<c r=\"{at}\"{s} t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
            escape(t)
        )
    };
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => format!("<c r=\"{at}\"{s} t=\"b\"><v>{}</v></c>", u8::from(*b)),
        Value::Number(n) if n.as_f64().is_some_and(f64::is_finite) => {
            format!("<c r=\"{at}\"{s}><v>{n}</v></c>")
        }
        Value::String(t) => text(t),
        other => text(&forge_sql::value::display(other)),
    }
}

/// The workbook of a result set.
pub fn workbook(sheet: &str, columns: &[String], rows: &[Vec<Value>]) -> Vec<u8> {
    let mut data = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetViews><sheetView workbookViewId=\"0\"><pane ySplit=\"1\" topLeftCell=\"A2\" activePane=\"bottomLeft\" state=\"frozen\"/></sheetView></sheetViews><sheetData>",
    );
    data.push_str("<row r=\"1\">");
    for (c, name) in columns.iter().enumerate() {
        data.push_str(&cell(0, c, &Value::String(name.clone()), 1));
    }
    data.push_str("</row>");
    for (r, row) in rows.iter().enumerate() {
        data.push_str(&format!("<row r=\"{}\">", r + 2));
        for (c, v) in row.iter().enumerate() {
            data.push_str(&cell(r + 1, c, v, 0));
        }
        data.push_str("</row>");
    }
    data.push_str("</sheetData></worksheet>");
    let sheet: String = sheet
        .chars()
        .filter(|c| !matches!(c, '\\' | '/' | '?' | '*' | '[' | ']' | ':'))
        .take(31)
        .collect();
    let sheet = if sheet.trim().is_empty() {
        "Sheet1".into()
    } else {
        sheet
    };
    let content_types = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/><Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/></Types>";
    let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>";
    let workbook = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets><sheet name=\"{}\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>",
        escape(&sheet)
    );
    let wb_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>";
    let styles = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><fonts count=\"2\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font><font><b/><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts><fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills><borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs><cellXfs count=\"2\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/><xf numFmtId=\"0\" fontId=\"1\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/></cellXfs></styleSheet>";
    zip(&[
        ("[Content_Types].xml", content_types.as_bytes().to_vec()),
        ("_rels/.rels", rels.as_bytes().to_vec()),
        ("xl/workbook.xml", workbook.into_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes().to_vec()),
        ("xl/styles.xml", styles.as_bytes().to_vec()),
        ("xl/worksheets/sheet1.xml", data.into_bytes()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_and_letters() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(column_letter(0), "A");
        assert_eq!(column_letter(25), "Z");
        assert_eq!(column_letter(26), "AA");
        assert_eq!(column_letter(701), "ZZ");
        assert_eq!(column_letter(702), "AAA");
    }

    #[test]
    fn writes_a_zip_with_the_parts() {
        let bytes = workbook(
            "orders",
            &["id".into(), "name".into()],
            &[vec![serde_json::json!(1), serde_json::json!("a<b")]],
        );
        assert_eq!(&bytes[..4], b"PK\x03\x04");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("xl/worksheets/sheet1.xml"));
        assert!(text.contains("<v>1</v>"));
        assert!(text.contains("a&lt;b"));
        assert!(text.contains("<sheet name=\"orders\""));
    }

    /// The workbook reads back with calamine (as Excel and LibreOffice read it).
    #[cfg(feature = "xlsx")]
    #[test]
    fn round_trips_through_the_reader() {
        let bytes = workbook(
            "Data",
            &["id".into(), "name".into(), "ok".into()],
            &[
                vec![
                    serde_json::json!(1),
                    serde_json::json!("Zoë"),
                    serde_json::json!(true),
                ],
                vec![
                    serde_json::json!(2.5),
                    Value::Null,
                    serde_json::json!(false),
                ],
            ],
        );
        let t = crate::read::read(
            &bytes,
            &crate::read::ReadOptions {
                format: crate::read::SourceFormat::Excel,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(t.sheets, ["Data"]);
        assert_eq!(t.columns, ["id", "name", "ok"]);
        assert_eq!(
            t.rows[0],
            [Some("1".into()), Some("Zoë".into()), Some("true".into())]
        );
        assert_eq!(t.rows[1], [Some("2.5".into()), None, Some("false".into())]);
    }
}
