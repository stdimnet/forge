//! CVSS 3.x base scores from a vector (`CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H`), for
//! advisories that carry a vector but no rating (cargo audit, OSV).

use crate::model::Severity;

/// The base score of a CVSS 3.0 / 3.1 vector; `None` for other versions or a broken vector.
pub fn base_score(vector: &str) -> Option<f64> {
    let mut parts = vector.trim().split('/');
    let version = parts.next()?;
    if version != "CVSS:3.1" && version != "CVSS:3.0" {
        return None;
    }
    let mut m = std::collections::HashMap::new();
    for p in parts {
        let (k, v) = p.split_once(':')?;
        m.insert(k, v);
    }
    let changed = match *m.get("S")? {
        "U" => false,
        "C" => true,
        _ => return None,
    };
    let av = match *m.get("AV")? {
        "N" => 0.85,
        "A" => 0.62,
        "L" => 0.55,
        "P" => 0.2,
        _ => return None,
    };
    let ac = match *m.get("AC")? {
        "L" => 0.77,
        "H" => 0.44,
        _ => return None,
    };
    let pr = match (*m.get("PR")?, changed) {
        ("N", _) => 0.85,
        ("L", false) => 0.62,
        ("L", true) => 0.68,
        ("H", false) => 0.27,
        ("H", true) => 0.5,
        _ => return None,
    };
    let ui = match *m.get("UI")? {
        "N" => 0.85,
        "R" => 0.62,
        _ => return None,
    };
    let cia = |k: &str| -> Option<f64> {
        match *m.get(k)? {
            "H" => Some(0.56),
            "L" => Some(0.22),
            "N" => Some(0.0),
            _ => None,
        }
    };
    let iss = 1.0 - (1.0 - cia("C")?) * (1.0 - cia("I")?) * (1.0 - cia("A")?);
    let impact = if changed {
        7.52 * (iss - 0.029) - 3.25 * (iss - 0.02f64).powi(15)
    } else {
        6.42 * iss
    };
    if impact <= 0.0 {
        return Some(0.0);
    }
    let exploitability = 8.22 * av * ac * pr * ui;
    let raw = if changed {
        1.08 * (impact + exploitability)
    } else {
        impact + exploitability
    };
    Some(round_up(raw.min(10.0)))
}

/// CVSS 3.1 `Roundup`: the smallest number with one decimal that is not below `x`.
fn round_up(x: f64) -> f64 {
    let int = (x * 100_000.0).round() as i64;
    if int % 10_000 == 0 {
        int as f64 / 100_000.0
    } else {
        ((int / 10_000) + 1) as f64 / 10.0
    }
}

/// The rating of a vector, `Unknown` when it cannot be scored.
pub fn severity(vector: &str) -> Severity {
    base_score(vector)
        .map(Severity::from_score)
        .unwrap_or(Severity::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_vectors() {
        assert_eq!(
            base_score("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H"),
            Some(9.8)
        );
        assert_eq!(
            base_score("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:C/C:H/I:H/A:H"),
            Some(10.0)
        );
        assert_eq!(
            base_score("CVSS:3.1/AV:N/AC:H/PR:N/UI:N/S:U/C:H/I:N/A:N"),
            Some(5.9)
        );
        assert_eq!(
            base_score("CVSS:3.0/AV:L/AC:L/PR:L/UI:N/S:U/C:N/I:N/A:N"),
            Some(0.0)
        );
        assert_eq!(base_score("CVSS:4.0/AV:N/AC:L"), None);
        assert_eq!(
            severity("CVSS:3.1/AV:N/AC:H/PR:N/UI:N/S:U/C:H/I:N/A:N"),
            Severity::Moderate
        );
    }
}
