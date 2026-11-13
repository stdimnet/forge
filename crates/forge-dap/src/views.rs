//! Data of the native views (Memory, Disassembly), computed here so the UI only renders.

use serde::{Deserialize, Serialize};

use crate::error::{DapError, DapResult};

/// One line of the Memory view: address, hex bytes and their ASCII.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRow {
    /// `0x00007ffd1000`.
    pub address: String,
    /// Two hex digits per byte; `??` for unreadable bytes.
    pub hex: Vec<String>,
    /// Printable ASCII, `.` for everything else.
    pub ascii: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryBlock {
    /// Address of the first byte.
    pub address: String,
    pub rows: Vec<MemoryRow>,
    /// Bytes the adapter could not read.
    pub unreadable: u32,
}

/// One instruction of the Disassembly view.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instruction {
    pub address: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<String>,
    pub instruction: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Project-relative source file, when mapped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// The instruction pointer of the selected frame.
    #[serde(default)]
    pub current: bool,
}

/// `0x1f`, `0X1F` or decimal `31`.
pub fn parse_address(s: &str) -> Option<u64> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

pub fn format_address(a: u64) -> String {
    if a > 0xffff_ffff {
        format!("0x{a:016x}")
    } else {
        format!("0x{a:08x}")
    }
}

/// Rows of `per_row` bytes starting at `start`; the last `unreadable` positions show `??`.
pub fn memory_rows(start: u64, bytes: &[u8], unreadable: usize, per_row: usize) -> Vec<MemoryRow> {
    let per_row = per_row.clamp(1, 64);
    let total = bytes.len() + unreadable;
    let mut rows = Vec::new();
    let mut offset = 0;
    while offset < total {
        let mut hex = Vec::with_capacity(per_row);
        let mut ascii = String::with_capacity(per_row);
        for i in offset..(offset + per_row).min(total) {
            match bytes.get(i) {
                Some(b) => {
                    hex.push(format!("{b:02x}"));
                    ascii.push(if b.is_ascii_graphic() || *b == b' ' {
                        *b as char
                    } else {
                        '.'
                    });
                }
                None => {
                    hex.push("??".into());
                    ascii.push(' ');
                }
            }
        }
        rows.push(MemoryRow {
            address: format_address(start.wrapping_add(offset as u64)),
            hex,
            ascii,
        });
        offset += per_row;
    }
    rows
}

/// Bytes typed in the Memory view: `de ad be ef`, `deadbeef` or `0xde,0xad`.
pub fn parse_hex_bytes(s: &str) -> DapResult<Vec<u8>> {
    let cleaned: String = s
        .split([' ', ',', '\n', '\t'])
        .map(|t| t.trim_start_matches("0x").trim_start_matches("0X"))
        .collect();
    if cleaned.is_empty() || !cleaned.len().is_multiple_of(2) {
        return Err(DapError::Other(format!("'{s}' is not a list of hex bytes")));
    }
    (0..cleaned.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&cleaned[i..i + 2], 16)
                .map_err(|_| DapError::Other(format!("'{s}' is not a list of hex bytes")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_show_hex_ascii_and_unreadable_bytes() {
        let rows = memory_rows(0x1000, b"forge\0\x01AB", 3, 8);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].address, "0x00001000");
        assert_eq!(rows[0].hex[..3], ["66", "6f", "72"]);
        assert_eq!(rows[0].ascii, "forge..A");
        assert_eq!(rows[1].address, "0x00001008");
        assert_eq!(rows[1].hex, ["42", "??", "??", "??"]);
        assert_eq!(format_address(0x7ffd_0000_1000), "0x00007ffd00001000");
    }

    #[test]
    fn parses_addresses_and_bytes() {
        assert_eq!(parse_address("0x1F"), Some(31));
        assert_eq!(parse_address(" 4096 "), Some(4096));
        assert_eq!(parse_address("main"), None);
        assert_eq!(
            parse_hex_bytes("de ad,0xbe EF").unwrap(),
            [0xde, 0xad, 0xbe, 0xef]
        );
        assert!(parse_hex_bytes("abc").is_err());
        assert!(parse_hex_bytes("zz").is_err());
    }
}
