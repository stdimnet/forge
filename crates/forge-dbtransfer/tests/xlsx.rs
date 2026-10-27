//! An Excel workbook written by openpyxl (as Excel and LibreOffice write them).
#![cfg(feature = "xlsx")]

use forge_dbtransfer::{preview, ReadOptions, SourceFormat, ValueKind};

const FIXTURE: &[u8] = include_bytes!("fixtures/customers.xlsx");

fn opts(sheet: &str) -> ReadOptions {
    ReadOptions {
        format: SourceFormat::Excel,
        sheet: sheet.into(),
        ..Default::default()
    }
}

#[test]
fn reads_the_first_sheet_with_types() {
    let p = preview(FIXTURE, &opts(""), 100).unwrap();
    let t = &p.table;
    assert_eq!(t.sheets, ["Customers", "Orders"]);
    assert_eq!(
        t.columns,
        ["id", "name", "joined", "balance", "vip", "note"]
    );
    assert_eq!(p.total_rows, 3);
    let row = |i: usize| t.rows[i].iter().map(|c| c.as_deref()).collect::<Vec<_>>();
    assert_eq!(
        row(0),
        [
            Some("1"),
            Some("Ada Lovelace"),
            Some("2024-03-01"),
            Some("1250.5"),
            Some("true"),
            None
        ]
    );
    assert_eq!(
        row(1),
        [
            Some("2"),
            Some("Grace Hopper"),
            Some("2024-03-02 14:30:00"),
            Some("0"),
            Some("false"),
            Some("multi\nline")
        ]
    );
    assert_eq!(
        row(2),
        [
            Some("3"),
            Some("Zoë"),
            None,
            Some("-7.25"),
            None,
            Some("x, \"y\"")
        ]
    );
    let kinds: Vec<ValueKind> = p.guesses.iter().map(|g| g.kind).collect();
    assert_eq!(
        kinds,
        [
            ValueKind::Integer,
            ValueKind::Text,
            ValueKind::DateTime,
            ValueKind::Decimal,
            ValueKind::Boolean,
            ValueKind::Text
        ]
    );
}

#[test]
fn reads_another_sheet_and_reports_missing_ones() {
    let p = preview(FIXTURE, &opts("Orders"), 100).unwrap();
    assert_eq!(p.table.columns, ["order", "customer"]);
    assert_eq!(
        p.table.rows,
        [vec![Some("100".to_string()), Some("1".to_string())]]
    );
    assert!(preview(FIXTURE, &opts("Nope"), 10).is_err());
    assert!(preview(b"not a workbook", &opts(""), 10).is_err());
}
