//! Where a manifest declares its dependencies: the package name and the version
//! constraint with their positions, read by each ecosystem's own parser where Forge has
//! one (`phpforge-composer`, `pyforge-lang-python`, `goforge-lang-go`, the JSON tree of
//! `forge-jsonschema`) and by small scanners for Cargo.toml, pom.xml and Gradle scripts.

use forge_jsonschema::json;
use forge_jsonschema::tree::Value;
use pyforge_lang_python::{deps, DeclaredIn};
use serde::{Deserialize, Serialize};

use crate::model::Ecosystem;
use crate::text::{Lines, Span};

/// The kinds of files Forge finds dependencies in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    ComposerJson,
    ComposerLock,
    PackageJson,
    Requirements,
    Pyproject,
    CargoToml,
    GoMod,
    Pom,
    Gradle,
}

impl FileKind {
    pub fn of(path: &str) -> Option<FileKind> {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        Some(match name {
            "composer.json" => Self::ComposerJson,
            "composer.lock" => Self::ComposerLock,
            "package.json" => Self::PackageJson,
            "pyproject.toml" => Self::Pyproject,
            "Cargo.toml" => Self::CargoToml,
            "go.mod" => Self::GoMod,
            "pom.xml" => Self::Pom,
            "build.gradle" | "build.gradle.kts" => Self::Gradle,
            _ if deps::is_requirements_file(path) => Self::Requirements,
            _ => return None,
        })
    }

    pub fn ecosystem(self) -> Ecosystem {
        match self {
            Self::ComposerJson | Self::ComposerLock => Ecosystem::Composer,
            Self::PackageJson => Ecosystem::Npm,
            Self::Requirements | Self::Pyproject => Ecosystem::Pypi,
            Self::CargoToml => Ecosystem::Cargo,
            Self::GoMod => Ecosystem::Go,
            Self::Pom | Self::Gradle => Ecosystem::Maven,
        }
    }
}

/// How a constraint is written, for the Upgrade quick fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Style {
    Composer,
    Npm,
    /// PEP 440 specifiers (requirements files, PEP 621, dependency groups).
    Pep440,
    /// Poetry's constraints (`^2.31`).
    Poetry,
    Cargo,
    Go,
    /// A plain version (pom.xml, Gradle).
    Exact,
}

/// One declared dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Declaration {
    pub package: String,
    pub name: Span,
    /// The constraint (without quotes); empty at the end of the name for a PEP 508
    /// requirement without one. `None` when it cannot be edited (lock files, workspace
    /// dependencies, URLs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Span>,
    pub constraint: String,
    pub style: Style,
}

/// The dependencies `path` declares.
pub fn declarations(path: &str, text: &str) -> Vec<Declaration> {
    match FileKind::of(path) {
        Some(FileKind::ComposerJson) => composer_json(text),
        Some(FileKind::ComposerLock) => composer_lock(text),
        Some(FileKind::PackageJson) => package_json(text),
        Some(FileKind::Requirements) => python(text, deps::requirements_txt(text)),
        Some(FileKind::Pyproject) => python(text, deps::pyproject(text)),
        Some(FileKind::CargoToml) => cargo_toml(text),
        Some(FileKind::GoMod) => go_mod(text),
        Some(FileKind::Pom) => pom(text),
        Some(FileKind::Gradle) => gradle(text),
        None => Vec::new(),
    }
}

/// The span inside the quotes of a JSON string node.
fn inner(lines: &Lines, start: usize, end: usize) -> Span {
    lines.span(start + 1, end.saturating_sub(1).max(start + 1))
}

fn composer_json(text: &str) -> Vec<Declaration> {
    let lines = Lines::new(text);
    phpforge_composer::manifest::requirements(text)
        .into_iter()
        .filter(|r| !r.is_platform())
        .map(|r| Declaration {
            name: inner(&lines, r.key.0, r.key.1),
            value: Some(inner(&lines, r.value.0, r.value.1)),
            constraint: r.constraint,
            package: r.name,
            style: Style::Composer,
        })
        .collect()
}

fn composer_lock(text: &str) -> Vec<Declaration> {
    let lines = Lines::new(text);
    phpforge_composer::lock::installed(Some(text), None)
        .into_iter()
        .filter_map(|p| {
            let (s, e) = phpforge_composer::manifest::lock_entry(text, &p.name)?;
            Some(Declaration {
                name: inner(&lines, s, e),
                value: None,
                constraint: p.version,
                package: p.name,
                style: Style::Composer,
            })
        })
        .collect()
}

fn package_json(text: &str) -> Vec<Declaration> {
    let Some(doc) = json::parse(text) else {
        return Vec::new();
    };
    let lines = Lines::new(text);
    let mut out = Vec::new();
    for section in [
        "dependencies",
        "devDependencies",
        "optionalDependencies",
        "peerDependencies",
    ] {
        let Some(Value::Object(props)) = doc.get(section).map(|n| &n.value) else {
            continue;
        };
        for p in props {
            let Some(v) = &p.value else { continue };
            let Value::String(constraint) = &v.value else {
                continue;
            };
            // Not on the registry: workspace packages, files, git, aliases.
            let editable = !constraint.contains(':') && !constraint.contains('/');
            out.push(Declaration {
                package: p.key.clone(),
                name: inner(&lines, p.key_start, p.key_end),
                value: editable.then(|| inner(&lines, v.start, v.end)),
                constraint: constraint.clone(),
                style: Style::Npm,
            });
        }
    }
    out
}

fn python(text: &str, file: deps::DepFile) -> Vec<Declaration> {
    let lines: Vec<&str> = text.lines().collect();
    file.deps
        .into_iter()
        .map(|d| {
            let name = Span {
                line: d.line,
                start: d.start,
                end: d.end,
            };
            let line = lines.get(d.line as usize).copied().unwrap_or("");
            let style = if d.source == DeclaredIn::Poetry {
                Style::Poetry
            } else {
                Style::Pep440
            };
            let value = if d.direct {
                None
            } else if style == Style::Poetry {
                poetry_value(line, d.line, d.end)
            } else {
                pep508_value(line, d.line, d.end)
            };
            Declaration {
                package: d.name,
                name,
                value,
                constraint: d.written,
                style,
            }
        })
        .collect()
}

fn col_to_byte(line: &str, col: u32) -> usize {
    let mut units = 0;
    for (i, c) in line.char_indices() {
        if units >= col {
            return i;
        }
        units += c.len_utf16() as u32;
    }
    line.len()
}

fn byte_to_col(line: &str, byte: usize) -> u32 {
    line[..byte.min(line.len())].encode_utf16().count() as u32
}

/// The specifiers after a PEP 508 name (`requests[socks] >=2.0 ; python_version…`).
fn pep508_value(line: &str, line_no: u32, name_end: u32) -> Option<Span> {
    let mut i = col_to_byte(line, name_end);
    let rest = &line[i..];
    let skip_ws = |s: &str| s.len() - s.trim_start().len();
    i += skip_ws(rest);
    if line[i..].starts_with('[') {
        i += line[i..].find(']')? + 1;
        i += skip_ws(&line[i..]);
    }
    if line[i..].starts_with('@') {
        return None;
    }
    let start = i;
    let mut end = line[start..]
        .find([';', '#', '"', '\''])
        .map(|e| start + e)
        .unwrap_or(line.len());
    if let Some(opt) = line[start..end].find(" --") {
        end = start + opt;
    }
    let end = start
        + line[start..end]
            .trim_end()
            .trim_end_matches('\\')
            .trim_end()
            .len();
    Some(Span {
        line: line_no,
        start: byte_to_col(line, start),
        end: byte_to_col(line, end),
    })
}

/// Poetry: `requests = "^2.31"` or `requests = { version = "^2.31", … }`.
fn poetry_value(line: &str, line_no: u32, name_end: u32) -> Option<Span> {
    let after = col_to_byte(line, name_end);
    let rest = &line[after..];
    let eq = rest.find('=')?;
    let mut from = after + eq + 1;
    let tail = line[from..].trim_start();
    if tail.starts_with('{') {
        let k = line[from..].find("version")?;
        from += k + "version".len();
    }
    let q = line[from..].find(['"', '\''])? + from;
    let quote = &line[q..q + 1];
    let close = line[q + 1..].find(quote)? + q + 1;
    Some(Span {
        line: line_no,
        start: byte_to_col(line, q + 1),
        end: byte_to_col(line, close),
    })
}

/// The span inside the first quoted string at or after byte `from` of `line`.
fn quoted(line: &str, from: usize) -> Option<(usize, usize)> {
    let q = line[from..].find(['"', '\''])? + from;
    let quote = &line[q..q + 1];
    let close = line[q + 1..].find(quote)? + q + 1;
    Some((q + 1, close))
}

/// Cargo.toml: `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`,
/// `[workspace.dependencies]`, their `[target.….dependencies]` forms and
/// `[dependencies.<name>]` tables; `package = "…"` renames.
fn cargo_toml(text: &str) -> Vec<Declaration> {
    let is_deps = |h: &str| {
        h == "dependencies"
            || h == "dev-dependencies"
            || h == "build-dependencies"
            || h.ends_with(".dependencies")
            || h.ends_with(".dev-dependencies")
            || h.ends_with(".build-dependencies")
    };
    let mut out: Vec<Declaration> = Vec::new();
    // Inside a `[dependencies.<name>]` table: the index of its declaration.
    let mut table: Option<usize> = None;
    let mut in_deps = false;
    for (n, raw) in text.lines().enumerate() {
        let n = n as u32;
        let line = raw.trim_end_matches('\r');
        let t = line.trim_start();
        if t.starts_with('[') {
            let header = t
                .trim_start_matches('[')
                .split(']')
                .next()
                .unwrap_or("")
                .trim();
            table = None;
            in_deps = is_deps(header);
            if !in_deps {
                if let Some((h, name)) = header.rsplit_once('.') {
                    if is_deps(h) {
                        let name = name.trim().trim_matches('"');
                        let start = line.rfind(name).unwrap_or(0);
                        out.push(Declaration {
                            package: name.to_string(),
                            name: Span {
                                line: n,
                                start: byte_to_col(line, start),
                                end: byte_to_col(line, start + name.len()),
                            },
                            value: None,
                            constraint: String::new(),
                            style: Style::Cargo,
                        });
                        table = Some(out.len() - 1);
                    }
                }
            }
            continue;
        }
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let Some(eq) = line.find('=') else { continue };
        let key_raw = line[..eq].trim();
        if let Some(k) = table {
            match key_raw {
                "version" => {
                    if let Some((s, e)) = quoted(line, eq) {
                        out[k].constraint = line[s..e].to_string();
                        out[k].value = Some(Span {
                            line: n,
                            start: byte_to_col(line, s),
                            end: byte_to_col(line, e),
                        });
                    }
                }
                "package" => {
                    if let Some((s, e)) = quoted(line, eq) {
                        out[k].package = line[s..e].to_string();
                    }
                }
                _ => {}
            }
            continue;
        }
        if !in_deps {
            continue;
        }
        // `name.workspace = true`.
        let key = key_raw
            .split('.')
            .next()
            .unwrap_or(key_raw)
            .trim()
            .trim_matches('"');
        if key.is_empty() {
            continue;
        }
        let key_start = line.find(key).unwrap_or(0);
        let mut d = Declaration {
            package: key.to_string(),
            name: Span {
                line: n,
                start: byte_to_col(line, key_start),
                end: byte_to_col(line, key_start + key.len()),
            },
            value: None,
            constraint: String::new(),
            style: Style::Cargo,
        };
        let rhs = line[eq + 1..].trim_start();
        let value_at = |from: usize| {
            quoted(line, from).map(|(s, e)| {
                (
                    line[s..e].to_string(),
                    Span {
                        line: n,
                        start: byte_to_col(line, s),
                        end: byte_to_col(line, e),
                    },
                )
            })
        };
        if rhs.starts_with('"') || rhs.starts_with('\'') {
            if let Some((c, span)) = value_at(eq + 1) {
                d.constraint = c;
                d.value = Some(span);
            }
        } else if rhs.starts_with('{') {
            if let Some(v) = line[eq..].find("version").map(|v| eq + v) {
                if let Some((c, span)) = line[v..].find('=').and_then(|e| value_at(v + e + 1)) {
                    d.constraint = c;
                    d.value = Some(span);
                }
            }
            if let Some(p) = line[eq..].find("package").map(|p| eq + p) {
                if let Some((c, _)) = line[p..].find('=').and_then(|e| value_at(p + e + 1)) {
                    d.package = c;
                }
            }
        }
        out.push(d);
    }
    out
}

fn word_span(w: &goforge_lang_go::gomod::Word) -> Span {
    Span {
        line: w.line,
        start: w.start,
        end: w.end,
    }
}

/// go.mod: the requirements, plus the `go` directive for the standard library
/// (`stdlib`) and `toolchain` for the Go toolchain.
fn go_mod(text: &str) -> Vec<Declaration> {
    let m = goforge_lang_go::parse_go_mod(text);
    let mut out: Vec<Declaration> = m
        .requires
        .iter()
        .map(|r| Declaration {
            package: r.path.text.clone(),
            name: word_span(&r.path),
            value: r.version.as_ref().map(word_span),
            constraint: r
                .version
                .as_ref()
                .map(|v| v.text.clone())
                .unwrap_or_default(),
            style: Style::Go,
        })
        .collect();
    for (package, word) in [("stdlib", &m.go), ("toolchain", &m.toolchain)] {
        if let Some(w) = word {
            out.push(Declaration {
                package: package.into(),
                name: word_span(w),
                value: None,
                constraint: w.text.clone(),
                style: Style::Go,
            });
        }
    }
    out
}

/// The text between `<tag>` and `</tag>` in `block` (byte range relative to `block`).
fn tag(block: &str, name: &str) -> Option<(usize, usize)> {
    let open = format!("<{name}>");
    let s = block.find(&open)? + open.len();
    let e = block[s..].find("</")? + s;
    Some((s, e))
}

/// pom.xml: each `<dependency>` as `groupId:artifactId`; a literal `<version>` can be
/// upgraded (a `${property}` cannot).
fn pom(text: &str) -> Vec<Declaration> {
    let lines = Lines::new(text);
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(k) = text[from..].find("<dependency>") {
        let start = from + k;
        let end = text[start..]
            .find("</dependency>")
            .map(|e| start + e)
            .unwrap_or(text.len());
        let block = &text[start..end];
        from = end;
        let (Some(g), Some(a)) = (tag(block, "groupId"), tag(block, "artifactId")) else {
            continue;
        };
        let group = block[g.0..g.1].trim();
        let artifact = block[a.0..a.1].trim();
        let version = tag(block, "version");
        let literal = version.filter(|v| !block[v.0..v.1].contains("${"));
        out.push(Declaration {
            package: format!("{group}:{artifact}"),
            name: lines.span(start + a.0, start + a.1),
            value: literal.map(|v| lines.span(start + v.0, start + v.1)),
            constraint: version
                .map(|v| block[v.0..v.1].trim().to_string())
                .unwrap_or_default(),
            style: Style::Exact,
        });
    }
    out
}

/// Gradle: `"group:artifact:version"` strings (Groovy and Kotlin DSL).
fn gradle(text: &str) -> Vec<Declaration> {
    let lines = Lines::new(text);
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let q = bytes[i];
        if q != b'"' && q != b'\'' {
            i += 1;
            continue;
        }
        let Some(len) = text[i + 1..].find(q as char) else {
            break;
        };
        let s = i + 1;
        let e = s + len;
        let lit = &text[s..e];
        i = e + 1;
        let parts: Vec<&str> = lit.split(':').collect();
        if parts.len() < 3
            || parts
                .iter()
                .any(|p| p.is_empty() || p.contains(char::is_whitespace))
            || lit.contains('$')
        {
            continue;
        }
        let ga = parts[0].len() + 1 + parts[1].len();
        let v_start = s + ga + 1;
        let v_end = v_start + parts[2].len();
        out.push(Declaration {
            package: format!("{}:{}", parts[0], parts[1]),
            name: lines.span(s, s + ga),
            value: Some(lines.span(v_start, v_end)),
            constraint: parts[2].to_string(),
            style: Style::Exact,
        });
    }
    out
}

/// The constraint that requires at least `fixed`, written like `old`; `None` when it
/// cannot be expressed (aliases, URLs).
pub fn upgraded_constraint(style: Style, old: &str, fixed: &str) -> Option<String> {
    let old = old.trim();
    let fixed = fixed.trim().trim_start_matches(['v', '=']);
    if fixed.is_empty() {
        return None;
    }
    let caret = |default: &str| -> String {
        for prefix in ["^", "~>", "~", ">="] {
            if old.starts_with(prefix) {
                return format!("{prefix}{fixed}");
            }
        }
        format!("{default}{fixed}")
    };
    Some(match style {
        Style::Composer => caret("^"),
        Style::Npm => {
            if old.starts_with(|c: char| c.is_ascii_digit()) && !old.contains([' ', '|', 'x', '*'])
            {
                fixed.to_string()
            } else {
                caret("^")
            }
        }
        Style::Cargo => {
            if old.starts_with('=') && !old.starts_with("==") {
                format!("={fixed}")
            } else if old.starts_with(|c: char| c.is_ascii_digit()) || old.is_empty() {
                fixed.to_string()
            } else {
                caret("^")
            }
        }
        Style::Pep440 => {
            if old.starts_with("==") {
                format!("=={fixed}")
            } else if old.starts_with("~=") {
                format!("~={fixed}")
            } else {
                format!(">={fixed}")
            }
        }
        Style::Poetry => caret(">="),
        Style::Go => format!("v{fixed}"),
        Style::Exact => fixed.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str, span: Span) -> &str {
        let line = text.lines().nth(span.line as usize).unwrap();
        &line[col_to_byte(line, span.start)..col_to_byte(line, span.end)]
    }

    fn summary(path: &str, text: &str) -> Vec<(String, String, Option<String>)> {
        declarations(path, text)
            .into_iter()
            .map(|d| {
                (
                    d.package.clone(),
                    at(text, d.name).to_string(),
                    d.value.map(|v| at(text, v).to_string()),
                )
            })
            .collect()
    }

    fn row(p: &str, n: &str, v: Option<&str>) -> (String, String, Option<String>) {
        (p.into(), n.into(), v.map(str::to_string))
    }

    #[test]
    fn finds_composer_and_npm_dependencies() {
        let composer = "{\n  \"require\": {\n    \"php\": \"^8.3\",\n    \"guzzlehttp/psr7\": \"^2.0\"\n  }\n}";
        assert_eq!(
            summary("composer.json", composer),
            [row("guzzlehttp/psr7", "guzzlehttp/psr7", Some("^2.0"))]
        );
        let lock = "{\n  \"packages\": [\n    {\"name\": \"guzzlehttp/psr7\", \"version\": \"2.4.4\"}\n  ]\n}";
        let d = declarations("composer.lock", lock);
        assert_eq!(d[0].constraint, "2.4.4");
        assert_eq!(d[0].value, None);
        let pkg = "{\n  \"dependencies\": {\n    \"lodash\": \"^4.17.20\",\n    \"icons\": \"workspace:*\"\n  },\n  \"devDependencies\": {\"vite\": \"6.3.5\"}\n}";
        assert_eq!(
            summary("package.json", pkg),
            [
                row("lodash", "lodash", Some("^4.17.20")),
                row("icons", "icons", None),
                row("vite", "vite", Some("6.3.5")),
            ]
        );
    }

    #[test]
    fn finds_python_dependencies() {
        let req =
            "requests[socks] >= 2.25 ; python_version > '3.8'\nflask==2.2.0  # web\nurllib3\n";
        assert_eq!(
            summary("requirements.txt", req),
            [
                row("requests", "requests", Some(">= 2.25")),
                row("flask", "flask", Some("==2.2.0")),
                row("urllib3", "urllib3", Some("")),
            ]
        );
        let py = "[project]\nname = \"x\"\ndependencies = [\n  \"jinja2>=3.0,<4\",\n]\n\n[tool.poetry.dependencies]\npython = \"^3.11\"\naiohttp = { version = \"^3.8.0\", extras = [\"speedups\"] }\n";
        let s = summary("pyproject.toml", py);
        assert!(s.contains(&row("jinja2", "jinja2", Some(">=3.0,<4"))));
        assert!(s.contains(&row("aiohttp", "aiohttp", Some("^3.8.0"))));
    }

    #[test]
    fn finds_cargo_dependencies() {
        let toml = "[package]\nname = \"x\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = \"1.0\"\nrsa = { version = \"0.9.6\", default-features = false }\nalias = { package = \"time\", version = \"=0.3.1\" }\nshared.workspace = true\n\n[dependencies.tokio]\nversion = \"1.20\"\nfeatures = [\"full\"]\n\n[target.'cfg(unix)'.dev-dependencies]\nnix = \"0.26\"\n";
        assert_eq!(
            summary("Cargo.toml", toml),
            [
                row("serde", "serde", Some("1.0")),
                row("rsa", "rsa", Some("0.9.6")),
                row("time", "alias", Some("=0.3.1")),
                row("shared", "shared", None),
                row("tokio", "tokio", Some("1.20")),
                row("nix", "nix", Some("0.26")),
            ]
        );
    }

    #[test]
    fn finds_go_and_jvm_dependencies() {
        let gomod = "module example.com/app\n\ngo 1.22.1\n\nrequire (\n\tgolang.org/x/net v0.17.0\n\tgithub.com/gin-gonic/gin v1.9.0 // indirect\n)\n";
        assert_eq!(
            summary("go.mod", gomod),
            [
                row("golang.org/x/net", "golang.org/x/net", Some("v0.17.0")),
                row(
                    "github.com/gin-gonic/gin",
                    "github.com/gin-gonic/gin",
                    Some("v1.9.0")
                ),
                row("stdlib", "1.22.1", None),
            ]
        );
        let pom = "<project>\n  <dependencies>\n    <dependency>\n      <groupId>org.apache.logging.log4j</groupId>\n      <artifactId>log4j-core</artifactId>\n      <version>2.14.1</version>\n    </dependency>\n    <dependency>\n      <groupId>g</groupId>\n      <artifactId>a</artifactId>\n      <version>${a.version}</version>\n    </dependency>\n  </dependencies>\n</project>\n";
        assert_eq!(
            summary("pom.xml", pom),
            [
                row(
                    "org.apache.logging.log4j:log4j-core",
                    "log4j-core",
                    Some("2.14.1")
                ),
                row("g:a", "a", None),
            ]
        );
        let gradle = "dependencies {\n    implementation(\"com.fasterxml.jackson.core:jackson-databind:2.9.8\")\n    implementation 'org.slf4j:slf4j-api:$v'\n}\n";
        assert_eq!(
            summary("build.gradle.kts", gradle),
            [row(
                "com.fasterxml.jackson.core:jackson-databind",
                "com.fasterxml.jackson.core:jackson-databind",
                Some("2.9.8")
            )]
        );
    }

    #[test]
    fn writes_upgraded_constraints_like_the_old_ones() {
        let up = |style, old| upgraded_constraint(style, old, "2.4.5").unwrap();
        assert_eq!(up(Style::Composer, "^2.0"), "^2.4.5");
        assert_eq!(up(Style::Composer, "2.0.*"), "^2.4.5");
        assert_eq!(up(Style::Npm, "~2.0.1"), "~2.4.5");
        assert_eq!(up(Style::Npm, "2.0.1"), "2.4.5");
        assert_eq!(up(Style::Npm, "*"), "^2.4.5");
        assert_eq!(up(Style::Cargo, "2.0"), "2.4.5");
        assert_eq!(up(Style::Cargo, "=2.0.1"), "=2.4.5");
        assert_eq!(up(Style::Pep440, "==2.0.1"), "==2.4.5");
        assert_eq!(up(Style::Pep440, ">=2,<3"), ">=2.4.5");
        assert_eq!(up(Style::Pep440, ""), ">=2.4.5");
        assert_eq!(up(Style::Poetry, "^2.0"), "^2.4.5");
        assert_eq!(
            upgraded_constraint(Style::Go, "v0.17.0", "v0.23.0").unwrap(),
            "v0.23.0"
        );
        assert_eq!(up(Style::Exact, "2.0"), "2.4.5");
        assert_eq!(upgraded_constraint(Style::Npm, "^1", " "), None);
    }
}
