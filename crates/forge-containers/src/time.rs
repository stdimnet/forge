//! Timestamps: RFC 3339 parsing (Kubernetes, volumes) and the "5 minutes ago" texts.

/// Seconds since the Unix epoch of an RFC 3339 timestamp (`2026-10-20T09:12:01Z`,
/// `2026-10-20T09:12:01.123456789+02:00`). Fractions are dropped.
pub fn parse_rfc3339(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.len() < 19 {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hour, min, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !matches!(s.as_bytes()[10], b'T' | b't' | b' ') || !(1..=12).contains(&month) {
        return None;
    }
    let mut rest = &s[19..];
    if let Some(r) = rest.strip_prefix('.') {
        rest = r.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    let offset = match rest {
        "" | "Z" | "z" => 0,
        r if r.len() == 6 && (r.starts_with('+') || r.starts_with('-')) => {
            let h: i64 = r[1..3].parse().ok()?;
            let m: i64 = r[4..6].parse().ok()?;
            let o = h * 3600 + m * 60;
            if r.starts_with('-') {
                -o
            } else {
                o
            }
        }
        _ => return None,
    };
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + min * 60 + sec - offset)
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `2026-10-20 09:12:01` (UTC) for a Unix timestamp.
pub fn format_utc(ts: i64) -> String {
    let days = ts.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

/// Kubernetes-style short age: `45s`, `12m`, `5h`, `3d`, `2y`.
pub fn short_age(created: i64, now: i64) -> String {
    let s = (now - created).max(0);
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m", s / 60),
        3600..=86_399 => format!("{}h", s / 3600),
        86_400..=31_535_999 => format!("{}d", s / 86_400),
        _ => format!("{}y", s / 31_536_000),
    }
}

/// Docker-style long age: `Less than a second`, `5 minutes`, `About an hour`, `3 days`.
pub fn long_age(created: i64, now: i64) -> String {
    let s = (now - created).max(0);
    let plural = |n: i64, unit: &str| format!("{n} {unit}{}", if n == 1 { "" } else { "s" });
    match s {
        0 => "Less than a second".into(),
        1..=59 => plural(s, "second"),
        60..=3599 => {
            if s < 120 {
                "About a minute".into()
            } else {
                plural(s / 60, "minute")
            }
        }
        3600..=86_399 => {
            if s < 7200 {
                "About an hour".into()
            } else {
                plural(s / 3600, "hour")
            }
        }
        86_400..=1_209_599 => plural(s / 86_400, "day"),
        1_209_600..=5_183_999 => plural(s / 604_800, "week"),
        5_184_000..=63_071_999 => plural(s / 2_592_000, "month"),
        _ => plural(s / 31_536_000, "year"),
    }
}

/// `1.2 GB`, `34.5 MB`, `512 B` (decimal units, like `docker images`).
pub fn format_size(bytes: i64) -> String {
    let b = bytes.max(0) as f64;
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    let mut v = b;
    let mut i = 0;
    while v >= 1000.0 && i < UNITS.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    if i == 0 {
        format!("{} B", bytes.max(0))
    } else if v >= 100.0 {
        format!("{v:.0} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rfc3339() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339("2026-10-20T09:12:01Z"), Some(1_792_487_521));
        assert_eq!(
            parse_rfc3339("2026-10-20T11:12:01.123456789+02:00"),
            Some(1_792_487_521)
        );
        assert_eq!(parse_rfc3339("not a date"), None);
        assert_eq!(format_utc(1_792_487_521), "2026-10-20 09:12:01");
    }

    #[test]
    fn ages_and_sizes() {
        assert_eq!(short_age(0, 30), "30s");
        assert_eq!(short_age(0, 7200), "2h");
        assert_eq!(short_age(0, 3 * 86_400), "3d");
        assert_eq!(long_age(0, 90), "About a minute");
        assert_eq!(long_age(0, 5 * 3600), "5 hours");
        assert_eq!(long_age(0, 86_400), "1 day");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1_234_567), "1.2 MB");
        assert_eq!(format_size(612_000_000), "612 MB");
    }
}
