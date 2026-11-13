//! Number and date formatting per locale (the formats PHPStorm's UI shows: grouped
//! decimals, short/medium/long dates and times).

use crate::plural::language;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberStyle {
    /// Up to three fraction digits, grouped ("1,234.5").
    Decimal,
    /// Rounded to a whole number, grouped.
    Integer,
    /// Multiplied by 100 and rounded ("25%").
    Percent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateStyle {
    Short,
    Medium,
    Long,
}

/// Decimal and grouping separators of a locale.
pub fn separators(locale: &str) -> (char, char) {
    match language(locale) {
        "de" | "es" | "it" | "nl" | "pt" | "tr" | "id" | "da" | "el" => (',', '.'),
        "fr" => (',', '\u{202f}'),
        "ru" | "uk" | "pl" | "cs" | "sk" | "sv" | "fi" | "nb" | "hu" => (',', '\u{a0}'),
        _ => ('.', ','),
    }
}

/// Format `n` in `locale`.
pub fn number(locale: &str, n: f64, style: NumberStyle) -> String {
    if !n.is_finite() {
        return n.to_string();
    }
    let (value, digits) = match style {
        NumberStyle::Decimal => (n, 3),
        NumberStyle::Integer => (n, 0),
        NumberStyle::Percent => (n * 100.0, 0),
    };
    // Round half away from zero ("2.5" → "3"), as ICU does, before printing.
    let scale = 10f64.powi(digits as i32);
    let s = format!("{:.*}", digits, (value.abs() * scale).round() / scale);
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let frac = frac.trim_end_matches('0');
    let (decimal, group) = separators(locale);
    let mut out = String::new();
    if value < 0.0 && s.chars().any(|c| c.is_ascii_digit() && c != '0') {
        out.push('-');
    }
    let len = int.len();
    for (idx, c) in int.chars().enumerate() {
        if idx > 0 && (len - idx) % 3 == 0 {
            out.push(group);
        }
        out.push(c);
    }
    if !frac.is_empty() {
        out.push(decimal);
        out.push_str(frac);
    }
    if style == NumberStyle::Percent {
        match language(locale) {
            "de" | "fr" | "sv" | "nb" | "fi" | "cs" | "sk" | "ru" | "uk" | "es" => {
                out.push_str("\u{a0}%")
            }
            _ => out.push('%'),
        }
    }
    out
}

/// Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

const EN_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DE_MONTHS: [&str; 12] = [
    "Januar",
    "Februar",
    "März",
    "April",
    "Mai",
    "Juni",
    "Juli",
    "August",
    "September",
    "Oktober",
    "November",
    "Dezember",
];

/// Format a date (`time == false`) or a time of day of `epoch_ms`, shifted by
/// `tz_offset_minutes` east of UTC.
pub fn date_time(
    locale: &str,
    epoch_ms: i64,
    tz_offset_minutes: i32,
    style: DateStyle,
    time: bool,
) -> String {
    let local = epoch_ms.div_euclid(1000) + i64::from(tz_offset_minutes) * 60;
    let days = local.div_euclid(86_400);
    let secs = local.rem_euclid(86_400);
    let (h, min, s) = (secs / 3600, (secs / 60) % 60, secs % 60);
    let lang = language(locale);
    if time {
        let seconds = style != DateStyle::Short;
        return match lang {
            "en" => {
                let h12 = if h % 12 == 0 { 12 } else { h % 12 };
                let ampm = if h < 12 { "AM" } else { "PM" };
                if seconds {
                    format!("{h12}:{min:02}:{s:02} {ampm}")
                } else {
                    format!("{h12}:{min:02} {ampm}")
                }
            }
            _ => {
                if seconds {
                    format!("{h:02}:{min:02}:{s:02}")
                } else {
                    format!("{h:02}:{min:02}")
                }
            }
        };
    }
    let (y, m, d) = civil(days);
    let mi = (m - 1) as usize;
    match (lang, style) {
        ("en", DateStyle::Short) => format!("{m}/{d}/{:02}", y.rem_euclid(100)),
        ("en", DateStyle::Medium) => format!("{} {d}, {y}", &EN_MONTHS[mi][..3]),
        ("en", DateStyle::Long) => format!("{} {d}, {y}", EN_MONTHS[mi]),
        ("de", DateStyle::Short) => format!("{d:02}.{m:02}.{:02}", y.rem_euclid(100)),
        ("de", DateStyle::Medium) => format!("{d:02}.{m:02}.{y}"),
        ("de", DateStyle::Long) => format!("{d}. {} {y}", DE_MONTHS[mi]),
        // ISO 8601 for languages without their own patterns yet.
        _ => format!("{y}-{m:02}-{d:02}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouping_and_rounding() {
        assert_eq!(number("en", 0.0, NumberStyle::Decimal), "0");
        assert_eq!(number("en", 999.0, NumberStyle::Decimal), "999");
        assert_eq!(number("en", 1000.0, NumberStyle::Decimal), "1,000");
        assert_eq!(number("en", -1234.5, NumberStyle::Decimal), "-1,234.5");
        assert_eq!(number("de", 1234.5, NumberStyle::Decimal), "1.234,5");
        assert_eq!(number("en", 0.1234, NumberStyle::Decimal), "0.123");
        assert_eq!(number("de", 1234.5, NumberStyle::Integer), "1.235");
        assert_eq!(number("en", -0.0001, NumberStyle::Decimal), "0");
        assert_eq!(number("fr", 1234.0, NumberStyle::Decimal), "1\u{202f}234");
    }

    #[test]
    fn dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(-1), (1969, 12, 31));
        assert_eq!(civil(19_782), (2024, 2, 29));
        let ms = 1_793_804_645_000;
        assert_eq!(date_time("en", ms, 0, DateStyle::Short, false), "11/4/26");
        assert_eq!(date_time("de", ms, 0, DateStyle::Short, false), "04.11.26");
        assert_eq!(date_time("de", ms, 60, DateStyle::Medium, true), "16:04:05");
        assert_eq!(
            date_time("en", ms, -16 * 60, DateStyle::Short, true),
            "11:04 PM"
        );
        assert_eq!(
            date_time("en", ms, -16 * 60, DateStyle::Short, false),
            "11/3/26"
        );
        assert_eq!(date_time("ja", ms, 0, DateStyle::Long, false), "2026-11-04");
    }
}
