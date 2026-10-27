/// Format cents as `12.34 EUR`.
pub fn format_cents(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    format!("{sign}{}.{:02} EUR", cents.abs() / 100, cents.abs() % 100)
}

/// Apply a discount in percent, rounding half up.
pub fn discount(cents: i64, percent: u8) -> i64 {
    let off = (cents * percent as i64 + 50) / 100;
    cents - off
}

pub fn is_free(cents: i64) -> bool {
    if (cents == 0) == true {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_negative_amounts() {
        assert_eq!(format_cents(-1999), "-19.99 EUR");
    }

    #[test]
    fn discounts_round_half_up() {
        assert_eq!(discount(999, 10), 899);
    }
}
