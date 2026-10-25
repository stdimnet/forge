//! PEP 508 dependency specifications (`requests[socks]>=2.31,<3 ; python_version >= "3.9"`,
//! `mypkg @ https://…`) and PEP 503 name normalization.

use serde::{Deserialize, Serialize};

use crate::pep440::SpecifierSet;

/// PEP 503: lowercase, runs of `-`, `_` and `.` become one `-` (`Django_REST.framework` →
/// `django-rest-framework`).
pub fn normalize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut sep = false;
    for c in name.trim().chars() {
        if matches!(c, '-' | '_' | '.') {
            sep = true;
            continue;
        }
        if sep && !out.is_empty() {
            out.push('-');
        }
        sep = false;
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// Whether `name` is a valid project name.
pub fn is_name(name: &str) -> bool {
    let b = name.as_bytes();
    !b.is_empty()
        && b[0].is_ascii_alphanumeric()
        && b[b.len() - 1].is_ascii_alphanumeric()
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}

/// A parsed requirement. Ranges are byte offsets into the parsed text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    pub name: String,
    pub extras: Vec<String>,
    /// The version specifiers as written (`>=2.31,<3`), without parentheses.
    pub specifier: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    pub name_start: usize,
    pub name_end: usize,
    /// The specifiers' range (empty at the name's end when there are none).
    pub spec_start: usize,
    pub spec_end: usize,
}

impl Requirement {
    /// The specifier set (`Err` when a specifier is malformed).
    pub fn specifiers(&self) -> Result<SpecifierSet, String> {
        SpecifierSet::parse(&self.specifier)
    }

    pub fn normalized_name(&self) -> String {
        normalize(&self.name)
    }

    /// `name[extras]specifier` (without marker), as shown in messages.
    pub fn display(&self) -> String {
        let mut s = self.name.clone();
        if !self.extras.is_empty() {
            s.push_str(&format!("[{}]", self.extras.join(",")));
        }
        s.push_str(&self.specifier.replace(' ', ""));
        s
    }
}

/// Parse a requirement. Errors are worded for the editor.
pub fn parse(text: &str) -> Result<Requirement, String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let name_start = i;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || b"-_.".contains(&bytes[i])) {
        i += 1;
    }
    let name_end = i;
    let name = &text[name_start..name_end];
    if !is_name(name) {
        return Err(if name.is_empty() {
            "Package name expected".into()
        } else {
            format!("'{name}' is not a valid package name")
        });
    }
    let skip_ws = |i: &mut usize| {
        while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
            *i += 1;
        }
    };
    skip_ws(&mut i);
    let mut extras = Vec::new();
    if bytes.get(i) == Some(&b'[') {
        let close = text[i..]
            .find(']')
            .map(|p| p + i)
            .ok_or("Missing ']' after the extras")?;
        for e in text[i + 1..close].split(',') {
            let e = e.trim();
            if e.is_empty() {
                continue;
            }
            if !is_name(e) {
                return Err(format!("'{e}' is not a valid extra"));
            }
            extras.push(e.to_string());
        }
        i = close + 1;
        skip_ws(&mut i);
    }
    // Everything up to `;` is the version part (or `@ url`).
    let marker_at = text[i..].find(';').map(|p| p + i);
    let version_end = marker_at.unwrap_or(text.len());
    let mut url = None;
    let mut spec_start = name_end;
    let mut spec_end = name_end;
    let mut specifier = String::new();
    let part = &text[i..version_end];
    if let Some(rest) = part.trim_start().strip_prefix('@') {
        let u = rest.trim();
        if u.is_empty() {
            return Err("URL expected after '@'".into());
        }
        url = Some(u.to_string());
    } else if !part.trim().is_empty() {
        let lead = part.len() - part.trim_start().len();
        let mut body = part.trim();
        let mut start = i + lead;
        if let Some(inner) = body.strip_prefix('(') {
            body = inner
                .strip_suffix(')')
                .ok_or("Missing ')' after the version specifiers")?;
            start += 1;
        }
        let trimmed_lead = body.len() - body.trim_start().len();
        let body = body.trim();
        SpecifierSet::parse(body)?;
        spec_start = start + trimmed_lead;
        spec_end = spec_start + body.len();
        specifier = body.to_string();
    }
    let marker = marker_at
        .map(|m| text[m + 1..].trim().to_string())
        .filter(|m| !m.is_empty());
    Ok(Requirement {
        name: name.to_string(),
        extras,
        specifier,
        url,
        marker,
        name_start,
        name_end,
        spec_start,
        spec_end,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_names() {
        assert_eq!(normalize("Django_REST.framework"), "django-rest-framework");
        assert_eq!(normalize("PyYAML"), "pyyaml");
        assert_eq!(normalize("a--b__c"), "a-b-c");
        assert!(is_name("zope.interface"));
        assert!(!is_name("-bad"));
    }

    #[test]
    fn parses_requirements() {
        let text = "requests[socks, security] >=2.31,<3 ; python_version >= \"3.9\"";
        let r = parse(text).unwrap();
        assert_eq!(r.name, "requests");
        assert_eq!(r.extras, vec!["socks", "security"]);
        assert_eq!(r.specifier, ">=2.31,<3");
        assert_eq!(&text[r.spec_start..r.spec_end], ">=2.31,<3");
        assert_eq!(&text[r.name_start..r.name_end], "requests");
        assert_eq!(r.marker.as_deref(), Some("python_version >= \"3.9\""));
        assert_eq!(r.display(), "requests[socks,security]>=2.31,<3");

        let r = parse("Django (>=4.2, <5)").unwrap();
        assert_eq!(r.specifier, ">=4.2, <5");
        assert!(r.specifiers().unwrap().contains(&"4.2.11".parse_version()));

        let r = parse("mypkg @ https://example.com/mypkg-1.0.tar.gz").unwrap();
        assert_eq!(
            r.url.as_deref(),
            Some("https://example.com/mypkg-1.0.tar.gz")
        );
        assert_eq!(r.spec_start, r.name_end);

        let r = parse("  flask").unwrap();
        assert_eq!((r.name_start, r.name_end), (2, 7));
        assert!(r.specifier.is_empty());
    }

    #[test]
    fn reports_errors() {
        assert!(parse("").is_err());
        assert!(parse("requests >=").is_err());
        assert!(parse("requests[socks").is_err());
        assert!(parse("requests (>=1").is_err());
        assert!(parse("pkg @").is_err());
    }

    trait ParseVersion {
        fn parse_version(&self) -> crate::pep440::Version;
    }
    impl ParseVersion for str {
        fn parse_version(&self) -> crate::pep440::Version {
            crate::pep440::Version::parse(self).unwrap()
        }
    }
}
