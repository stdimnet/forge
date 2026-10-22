//! The basic `format` checks. Unknown formats (and ones as loose as `uri-reference`)
//! always pass, like most validators do by default.

use std::net::{Ipv4Addr, Ipv6Addr};

fn digits(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_digit())
}

fn date(s: &str) -> bool {
    let p: Vec<&str> = s.split('-').collect();
    if p.len() != 3 || !digits(p[0], 4) || !digits(p[1], 2) || !digits(p[2], 2) {
        return false;
    }
    let (m, d): (u32, u32) = (p[1].parse().unwrap_or(0), p[2].parse().unwrap_or(0));
    (1..=12).contains(&m) && (1..=31).contains(&d)
}

fn time(s: &str) -> bool {
    let s = s.trim_end_matches(['Z', 'z']);
    let (clock, _offset) = match s.rfind(['+', '-']) {
        Some(i) if i >= 8 => (&s[..i], Some(&s[i + 1..])),
        _ => (s, None),
    };
    let clock = clock.split('.').next().unwrap_or("");
    let p: Vec<&str> = clock.split(':').collect();
    p.len() == 3
        && p.iter().all(|x| digits(x, 2))
        && p[0].parse::<u32>().unwrap_or(99) < 24
        && p[1].parse::<u32>().unwrap_or(99) < 60
        && p[2].parse::<u32>().unwrap_or(99) < 61
}

pub fn check(format: &str, s: &str) -> bool {
    match format {
        "date" => date(s),
        "time" => time(s),
        "date-time" => s
            .split_once(['T', 't', ' '])
            .is_some_and(|(d, t)| date(d) && time(t)),
        "email" => s.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !s.contains(' ')
        }),
        "ipv4" => s.parse::<Ipv4Addr>().is_ok(),
        "ipv6" => s.parse::<Ipv6Addr>().is_ok(),
        "uuid" => {
            let p: Vec<&str> = s.split('-').collect();
            p.len() == 5
                && [8, 4, 4, 4, 12]
                    .iter()
                    .zip(&p)
                    .all(|(n, x)| x.len() == *n && x.bytes().all(|b| b.is_ascii_hexdigit()))
        }
        // `${…}`: templated values in CI files.
        "uri" => (crate::uri::is_absolute(s) && !s.contains(' ')) || s.starts_with("${"),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::check;

    #[test]
    fn checks_formats() {
        assert!(check("date", "2024-02-29"));
        assert!(!check("date", "2024-13-01"));
        assert!(check("date-time", "2024-02-29T10:00:00.5+01:00"));
        assert!(check("date-time", "2024-02-29T10:00:00Z"));
        assert!(!check("date-time", "yesterday"));
        assert!(check("email", "jordi@example.com"));
        assert!(!check("email", "jordi"));
        assert!(check("ipv4", "127.0.0.1"));
        assert!(!check("ipv6", "127.0.0.1"));
        assert!(check("uuid", "123e4567-e89b-12d3-a456-426614174000"));
        assert!(check("uri", "https://getcomposer.org"));
        assert!(!check("uri", "not a uri"));
        assert!(check("hostname", "anything"));
    }
}
