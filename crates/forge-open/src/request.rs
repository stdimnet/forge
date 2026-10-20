//! Open requests as they arrive from the browser extension, `forge://` links and the CLI,
//! plus their validation.

use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};

/// Longest accepted file reference (bytes).
pub const MAX_PATH_LEN: usize = 4096;
/// Longest accepted symbol name.
pub const MAX_SYMBOL_LEN: usize = 200;
/// Lines and columns above this are rejected as nonsense.
const MAX_POSITION: u32 = 10_000_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RequestError {
    #[error("file is missing")]
    MissingFile,
    #[error("file reference is too long")]
    TooLong,
    #[error("file reference contains invalid characters")]
    InvalidChars,
    #[error("invalid symbol name")]
    InvalidSymbol,
    #[error("invalid {0}")]
    InvalidNumber(&'static str),
    #[error("not a forge:// link: {0}")]
    NotALink(String),
    #[error("unknown link action: {0}")]
    UnknownAction(String),
}

/// "Open this file (at this position)".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRequest {
    /// Absolute path, `file://` URL, or a path relative to `project`.
    pub file: String,
    /// 1-based line.
    #[serde(default)]
    pub line: Option<u32>,
    /// 1-based column.
    #[serde(default)]
    pub column: Option<u32>,
    /// Project root the caller believes the file belongs to; resolves relative paths.
    #[serde(default)]
    pub project: Option<String>,
    /// Symbol whose declaration is used as position when `line` is unknown
    /// (e.g. a React component name).
    #[serde(default)]
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    Class,
    Component,
}

/// "Open the declaration of this symbol" for callers that only know a name, e.g. the
/// constructor name of an Angular component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolRequest {
    pub name: String,
    #[serde(default)]
    pub kind: Option<SymbolKind>,
}

/// What a `forge://` link asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkAction {
    Open(OpenRequest),
    OpenSymbol(SymbolRequest),
}

fn check_symbol(name: &str) -> Result<(), RequestError> {
    let ok = !name.is_empty()
        && name.len() <= MAX_SYMBOL_LEN
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        && !name.starts_with(|c: char| c.is_ascii_digit());
    if ok {
        Ok(())
    } else {
        Err(RequestError::InvalidSymbol)
    }
}

/// `0` means "unknown" (some tools send 0-based or empty values); huge values are rejected.
fn check_position(v: Option<u32>, what: &'static str) -> Result<Option<u32>, RequestError> {
    match v {
        Some(0) | None => Ok(None),
        Some(n) if n > MAX_POSITION => Err(RequestError::InvalidNumber(what)),
        Some(n) => Ok(Some(n)),
    }
}

/// Turn `file:///home/me/a%20b.php` into `/home/me/a b.php` and `file:///C:/x` into `C:/x`.
/// Other strings are returned unchanged.
pub fn strip_file_url(file: &str) -> String {
    let Some(rest) = file.strip_prefix("file://") else {
        return file.to_string();
    };
    // `file://localhost/path` is the same as `file:///path`.
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let decoded = percent_decode_str(rest).decode_utf8_lossy().into_owned();
    // `/C:/Users/...` → `C:/Users/...`
    let b = decoded.as_bytes();
    if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
        decoded[1..].to_string()
    } else {
        decoded
    }
}

impl OpenRequest {
    pub fn new(file: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            ..Self::default()
        }
    }

    pub fn at(mut self, line: Option<u32>, column: Option<u32>) -> Self {
        self.line = line;
        self.column = column;
        self
    }

    /// Check limits and normalize: trims the path, strips `file://`, drops zero positions.
    pub fn validate(mut self) -> Result<Self, RequestError> {
        let file = strip_file_url(self.file.trim());
        if file.is_empty() {
            return Err(RequestError::MissingFile);
        }
        if file.len() > MAX_PATH_LEN {
            return Err(RequestError::TooLong);
        }
        if file.chars().any(|c| c == '\0' || c == '\n' || c == '\r') {
            return Err(RequestError::InvalidChars);
        }
        self.file = file;
        self.line = check_position(self.line, "line")?;
        self.column = check_position(self.column, "column")?;
        if self.line.is_none() {
            self.column = None;
        }
        if let Some(p) = &self.project {
            let p = strip_file_url(p.trim());
            if p.len() > MAX_PATH_LEN || p.contains('\0') {
                return Err(RequestError::InvalidChars);
            }
            self.project = (!p.is_empty()).then_some(p);
        }
        if let Some(s) = &self.symbol {
            check_symbol(s)?;
        }
        Ok(self)
    }
}

impl SymbolRequest {
    pub fn validate(self) -> Result<Self, RequestError> {
        let name = self.name.trim().to_string();
        check_symbol(&name)?;
        Ok(Self { name, ..self })
    }
}

fn parse_number(v: &str, what: &'static str) -> Result<Option<u32>, RequestError> {
    let v = v.trim();
    if v.is_empty() {
        return Ok(None);
    }
    v.parse::<u32>()
        .map(Some)
        .map_err(|_| RequestError::InvalidNumber(what))
}

/// Parse `forge://open?file=/abs/path&line=12&column=3` (also `url=file://…`, `l`, `col`,
/// as used by TextMate-style editor links) and `forge://open-symbol?name=UserCard`.
///
/// Values are percent-decoded but `+` stays a plus: frameworks substitute raw paths into
/// link templates, and `+` is a valid file name character.
pub fn parse_forge_url(link: &str) -> Result<LinkAction, RequestError> {
    let link = link.trim();
    let rest = link
        .strip_prefix("forge://")
        .or_else(|| link.strip_prefix("forge:"))
        .ok_or_else(|| RequestError::NotALink(link.chars().take(60).collect()))?;
    let (action, query) = rest.split_once('?').unwrap_or((rest, ""));
    let action = action.trim_end_matches('/');

    let mut file = None;
    let mut line = None;
    let mut column = None;
    let mut project = None;
    let mut name = None;
    let mut kind = None;
    let mut symbol = None;
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let v = percent_decode_str(v).decode_utf8_lossy().into_owned();
        match k {
            "file" | "path" | "url" => file = Some(v),
            "line" | "l" => line = parse_number(&v, "line")?,
            "column" | "col" | "c" => column = parse_number(&v, "column")?,
            "project" => project = Some(v),
            "name" => name = Some(v),
            "symbol" => symbol = Some(v),
            "kind" => {
                kind = match v.as_str() {
                    "class" => Some(SymbolKind::Class),
                    "component" => Some(SymbolKind::Component),
                    _ => None,
                }
            }
            _ => {}
        }
    }

    match action {
        "open" | "" => Ok(LinkAction::Open(
            OpenRequest {
                file: file.ok_or(RequestError::MissingFile)?,
                line,
                column,
                project,
                symbol,
            }
            .validate()?,
        )),
        "open-symbol" => Ok(LinkAction::OpenSymbol(
            SymbolRequest {
                name: name.unwrap_or_default(),
                kind,
            }
            .validate()?,
        )),
        other => Err(RequestError::UnknownAction(
            other.chars().take(40).collect(),
        )),
    }
}

/// Split `path:12:3` / `path:12` into the path and its position. A Windows drive letter
/// (`C:\x`) is not mistaken for a position; only trailing numeric groups are taken.
pub fn split_position_suffix(arg: &str) -> (&str, Option<u32>, Option<u32>) {
    fn trailing_number(s: &str) -> Option<(&str, u32)> {
        let (head, tail) = s.rsplit_once(':')?;
        if head.is_empty() || tail.is_empty() || !tail.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some((head, tail.parse().ok()?))
    }
    match trailing_number(arg) {
        Some((head, last)) => match trailing_number(head) {
            Some((path, line)) => (path, Some(line), Some(last)),
            None => (head, Some(last), None),
        },
        None => (arg, None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_normalizes() {
        let r = OpenRequest {
            file: "  file:///home/me/My%20App/src/App.tsx ".into(),
            line: Some(12),
            column: Some(0),
            ..Default::default()
        }
        .validate()
        .unwrap();
        assert_eq!(r.file, "/home/me/My App/src/App.tsx");
        assert_eq!((r.line, r.column), (Some(12), None));

        let no_line = OpenRequest::new("/a").at(None, Some(4)).validate().unwrap();
        assert_eq!(
            no_line.column, None,
            "a column without a line is meaningless"
        );
    }

    #[test]
    fn rejects_bad_requests() {
        assert_eq!(
            OpenRequest::new("  ").validate(),
            Err(RequestError::MissingFile)
        );
        assert_eq!(
            OpenRequest::new("/a\0b").validate(),
            Err(RequestError::InvalidChars)
        );
        assert_eq!(
            OpenRequest::new("x".repeat(MAX_PATH_LEN + 1)).validate(),
            Err(RequestError::TooLong)
        );
        assert_eq!(
            OpenRequest::new("/a").at(Some(u32::MAX), None).validate(),
            Err(RequestError::InvalidNumber("line"))
        );
        let bad_symbol = OpenRequest {
            symbol: Some("a b".into()),
            ..OpenRequest::new("/a")
        };
        assert_eq!(bad_symbol.validate(), Err(RequestError::InvalidSymbol));
        assert!(SymbolRequest {
            name: "<script>".into(),
            kind: None
        }
        .validate()
        .is_err());
        assert!(SymbolRequest {
            name: " UserCardComponent ".into(),
            kind: Some(SymbolKind::Component)
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn windows_file_urls() {
        assert_eq!(strip_file_url("file:///C:/src/a.ts"), "C:/src/a.ts");
        assert_eq!(strip_file_url("file://localhost/srv/a.ts"), "/srv/a.ts");
        assert_eq!(strip_file_url("/plain"), "/plain");
    }

    #[test]
    fn parses_forge_links() {
        let LinkAction::Open(r) =
            parse_forge_url("forge://open?file=/home/me/app/src/a%2Bb.php&line=12&column=3")
                .unwrap()
        else {
            panic!("expected open")
        };
        assert_eq!(r.file, "/home/me/app/src/a+b.php");
        assert_eq!((r.line, r.column), (Some(12), Some(3)));

        // TextMate-style link as produced by some error pages.
        let LinkAction::Open(r) =
            parse_forge_url("forge://open/?url=file:///srv/app/x.php&l=7").unwrap()
        else {
            panic!("expected open")
        };
        assert_eq!((r.file.as_str(), r.line), ("/srv/app/x.php", Some(7)));

        // Raw substitution keeps `+` (not form-decoded to a space).
        let LinkAction::Open(r) = parse_forge_url("forge://open?file=/a/c++/x.cpp&line=").unwrap()
        else {
            panic!("expected open")
        };
        assert_eq!((r.file.as_str(), r.line), ("/a/c++/x.cpp", None));

        assert_eq!(
            parse_forge_url("forge://open-symbol?name=UserCardComponent&kind=component").unwrap(),
            LinkAction::OpenSymbol(SymbolRequest {
                name: "UserCardComponent".into(),
                kind: Some(SymbolKind::Component)
            })
        );
    }

    #[test]
    fn rejects_bad_links() {
        assert!(matches!(
            parse_forge_url("https://example.com"),
            Err(RequestError::NotALink(_))
        ));
        assert!(matches!(
            parse_forge_url("forge://delete?file=/a"),
            Err(RequestError::UnknownAction(_))
        ));
        assert_eq!(
            parse_forge_url("forge://open?line=3"),
            Err(RequestError::MissingFile)
        );
        assert_eq!(
            parse_forge_url("forge://open?file=/a&line=x"),
            Err(RequestError::InvalidNumber("line"))
        );
    }

    #[test]
    fn splits_position_suffixes() {
        assert_eq!(
            split_position_suffix("src/a.ts:12:3"),
            ("src/a.ts", Some(12), Some(3))
        );
        assert_eq!(
            split_position_suffix("src/a.ts:12"),
            ("src/a.ts", Some(12), None)
        );
        assert_eq!(split_position_suffix("src/a.ts"), ("src/a.ts", None, None));
        assert_eq!(
            split_position_suffix(r"C:\src\a.ts:9"),
            (r"C:\src\a.ts", Some(9), None)
        );
        assert_eq!(split_position_suffix(r"C:\src"), (r"C:\src", None, None));
        assert_eq!(split_position_suffix("a.ts:"), ("a.ts:", None, None));
    }
}
