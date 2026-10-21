//! Find the declaration of a symbol the browser only knows by name (an Angular component's
//! constructor name, a React component without debug source) in the open project.

use forge_core::{FileMatch, Result, Workspace};
use serde::Serialize;

use crate::request::{SymbolKind, SymbolRequest};

/// Framework suffixes stripped to get the base name (`UserCardComponent` → `UserCard`).
const SUFFIXES: &[&str] = &[
    "Component",
    "Directive",
    "Pipe",
    "Service",
    "Module",
    "Page",
    "View",
];

/// Source files a component or class can be declared in.
const SOURCE_EXTENSIONS: &[&str] = &[
    "ts", "tsx", "js", "jsx", "mjs", "vue", "svelte", "php", "astro",
];

/// Files larger than this are not scanned for declarations.
const MAX_SCAN_BYTES: u64 = 2 * 1024 * 1024;
/// File-name matches considered per query.
const MAX_CANDIDATES: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolHit {
    /// Workspace-relative path.
    pub path: String,
    /// 1-based declaration position, if the declaration was found in the file.
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub score: i32,
}

/// Lowercase letters and digits only: `user-card.component` → `usercardcomponent`.
pub fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// `UserCardComponent` → `Some("UserCard")`.
pub fn base_name(name: &str) -> Option<&str> {
    SUFFIXES
        .iter()
        .find_map(|s| name.strip_suffix(s))
        .filter(|b| !b.is_empty())
}

/// How well a file name matches a symbol, 0 for no match. `user-card.component.ts` and
/// `UserCardComponent.ts` match `UserCardComponent` fully; `UserCard.tsx` matches its base.
pub fn file_name_score(file_name: &str, symbol: &str) -> i32 {
    let mut parts: Vec<&str> = file_name.split('.').collect();
    if parts.len() > 1 {
        parts.pop(); // extension
    }
    let full = normalize(&parts.concat());
    let first = normalize(parts[0]);
    let want = normalize(symbol);
    let base = base_name(symbol).map(normalize);
    if full == want || first == want {
        100
    } else if base.as_deref() == Some(full.as_str()) {
        70
    } else if base.as_deref() == Some(first.as_str()) {
        40
    } else {
        0
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// 1-based line and column of `name`'s declaration (`class X`, `function X`, `const X =`,
/// `interface X`, ...), if `content` declares it.
pub fn find_declaration(content: &str, name: &str) -> Option<(u32, u32)> {
    const KEYWORDS: &[&str] = &[
        "class",
        "function",
        "function*",
        "const",
        "let",
        "var",
        "interface",
        "trait",
        "enum",
        "type",
        "struct",
    ];
    for (idx, line) in content.lines().enumerate() {
        let mut from = 0;
        while let Some(pos) = line[from..].find(name).map(|p| p + from) {
            from = pos + name.len();
            let before = line[..pos].chars().next_back();
            let after = line[pos + name.len()..].chars().next();
            if before.is_some_and(is_word_char) || after.is_some_and(is_word_char) {
                continue;
            }
            let prev_word = line[..pos].split_whitespace().next_back().unwrap_or("");
            if KEYWORDS.contains(&prev_word) {
                let col = line[..pos].chars().count() as u32 + 1;
                return Some((idx as u32 + 1, col));
            }
        }
    }
    None
}

fn has_source_extension(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, ext)| SOURCE_EXTENSIONS.contains(&ext))
}

fn kind_bonus(path: &str, kind: Option<SymbolKind>) -> i32 {
    let ext = path.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match kind {
        Some(SymbolKind::Component) if matches!(ext, "tsx" | "jsx" | "vue" | "svelte") => 10,
        Some(SymbolKind::Component) if path.contains(".component.") => 10,
        Some(SymbolKind::Class) if matches!(ext, "ts" | "php") => 5,
        _ => 0,
    }
}

/// The files of the open project: a local [`Workspace`], or a remote project's backend.
pub trait SymbolSource {
    /// "Go to File" matches for `query`.
    fn find_files(&self, query: &str, limit: usize) -> Result<Vec<FileMatch>>;
    /// A file's text when it exists and is at most `max_bytes` long.
    fn read_small(&self, path: &str, max_bytes: u64) -> Option<String>;
}

impl SymbolSource for Workspace {
    fn find_files(&self, query: &str, limit: usize) -> Result<Vec<FileMatch>> {
        Workspace::find_files(self, query, limit)
    }

    fn read_small(&self, path: &str, max_bytes: u64) -> Option<String> {
        self.resolve(path)
            .ok()
            .filter(|p| p.metadata().is_ok_and(|m| m.len() <= max_bytes))
            .and_then(|p| std::fs::read_to_string(p).ok())
    }
}

/// Candidate declarations of `request.name` in the workspace, best first.
pub fn resolve_symbol(
    ws: &(impl SymbolSource + ?Sized),
    request: &SymbolRequest,
) -> Result<Vec<SymbolHit>> {
    let name = request.name.as_str();
    let base = base_name(name).unwrap_or(name);
    let mut files: Vec<String> = ws
        .find_files(base, MAX_CANDIDATES)?
        .into_iter()
        .map(|m| m.path)
        .collect();
    // `UserCard/index.tsx` style components.
    files.extend(
        ws.find_files(&format!("{base}/index"), 50)?
            .into_iter()
            .map(|m| m.path),
    );
    files.sort();
    files.dedup();

    let mut hits = Vec::new();
    for path in files.into_iter().filter(|p| has_source_extension(p)) {
        let file_name = path.rsplit('/').next().unwrap_or(&path);
        let mut score = file_name_score(file_name, name);
        if score == 0 && file_name.starts_with("index.") {
            let dir = path.rsplit('/').nth(1).unwrap_or("");
            score = file_name_score(&format!("{dir}.x"), name) / 2;
        }
        if score == 0 {
            continue;
        }
        let declaration = ws
            .read_small(&path, MAX_SCAN_BYTES)
            .and_then(|content| find_declaration(&content, name));
        if declaration.is_some() {
            score += 50;
        }
        score += kind_bonus(&path, request.kind);
        hits.push(SymbolHit {
            line: declaration.map(|d| d.0),
            column: declaration.map(|d| d.1),
            path,
            score,
        });
    }
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.path.len().cmp(&b.path.len()))
            .then_with(|| a.path.cmp(&b.path))
    });
    hits.truncate(20);
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn file_names_match_symbols() {
        assert_eq!(
            file_name_score("user-card.component.ts", "UserCardComponent"),
            100
        );
        assert_eq!(file_name_score("UserCard.tsx", "UserCard"), 100);
        assert_eq!(file_name_score("UserCard.tsx", "UserCardComponent"), 70);
        assert_eq!(
            file_name_score("user-card.service.ts", "UserCardComponent"),
            40
        );
        assert_eq!(file_name_score("user.ts", "UserCardComponent"), 0);
    }

    #[test]
    fn finds_declarations() {
        let src = "import x from 'y';\n\n@Component({})\nexport class UserCardComponent implements OnInit {\n";
        assert_eq!(find_declaration(src, "UserCardComponent"), Some((4, 14)));
        assert_eq!(
            find_declaration("export default function UserCard() {}", "UserCard"),
            Some((1, 25))
        );
        assert_eq!(
            find_declaration("  const UserCard = () => <div/>;", "UserCard"),
            Some((1, 9))
        );
        // Usages and longer names are not declarations.
        assert_eq!(
            find_declaration("<UserCard />\nclass UserCardList {}", "UserCard"),
            None
        );
    }

    #[test]
    fn resolves_symbols_in_a_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let files = [
            (
                "src/app/user-card/user-card.component.ts",
                "@Component({selector: 'app-user-card'})\nexport class UserCardComponent {}\n",
            ),
            ("src/app/user-card/user-card.component.html", "<div></div>"),
            (
                "src/app/user-card/user-card.service.ts",
                "export class UserCardService {}",
            ),
            (
                "src/components/Avatar/index.tsx",
                "export function Avatar() { return null }",
            ),
            ("README.md", "UserCardComponent"),
        ];
        for (p, c) in files {
            let path = dir.path().join(p);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, c).unwrap();
        }
        let ws = Workspace::open(dir.path()).unwrap();

        let hits = resolve_symbol(
            &ws,
            &SymbolRequest {
                name: "UserCardComponent".into(),
                kind: Some(SymbolKind::Component),
            },
        )
        .unwrap();
        assert_eq!(hits[0].path, "src/app/user-card/user-card.component.ts");
        assert_eq!((hits[0].line, hits[0].column), (Some(2), Some(14)));
        assert!(hits.iter().all(|h| has_source_extension(&h.path)));

        let hits = resolve_symbol(
            &ws,
            &SymbolRequest {
                name: "Avatar".into(),
                kind: Some(SymbolKind::Component),
            },
        )
        .unwrap();
        assert_eq!(hits[0].path, "src/components/Avatar/index.tsx");
        assert_eq!(hits[0].line, Some(1));
    }
}
