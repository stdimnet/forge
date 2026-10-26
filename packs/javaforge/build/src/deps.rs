//! Dependencies a build file declares (Gradle Kotlin and Groovy DSL, `pom.xml`) with the
//! place of their version, and the "A newer version is available" inspection with its
//! quick fix (replace the version, or the Maven property that holds it).

use std::collections::HashMap;

use forge_jsonschema::text::Utf16;
use regex_lite::Regex;
use serde::{Deserialize, Serialize};

use crate::version;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    GradleKotlin,
    GradleGroovy,
    Pom,
}

impl FileKind {
    /// The kind of a build file by its name.
    pub fn of_path(path: &str) -> Option<FileKind> {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        if name.ends_with(".gradle.kts") {
            Some(FileKind::GradleKotlin)
        } else if name.ends_with(".gradle") {
            Some(FileKind::GradleGroovy)
        } else if name == "pom.xml" {
            Some(FileKind::Pom)
        } else {
            None
        }
    }
}

/// A declared dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Declared {
    /// Gradle configuration (`implementation`) or Maven scope (`compile` when unset).
    pub configuration: String,
    pub group: String,
    pub artifact: String,
    /// The version as written (`33.3.1-jre`, `${guava.version}`; empty when managed).
    pub version: String,
    /// 0-based line of the declaration.
    pub line: usize,
    /// Byte range of the version text to replace (the property's value for `${…}`).
    #[serde(skip)]
    pub version_range: Option<(usize, usize)>,
    /// The version's value after resolving a Maven property.
    #[serde(skip)]
    pub resolved: String,
}

const CONFIGURATIONS: &str = r"implementation|api|compileOnly|runtimeOnly|testImplementation|testCompileOnly|testRuntimeOnly|annotationProcessor|testAnnotationProcessor|compileOnlyApi|developmentOnly|kapt|ksp|classpath|[a-z][A-Za-z0-9]*(?:Implementation|Api|CompileOnly|RuntimeOnly|AnnotationProcessor)";

fn line_of(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].matches('\n').count()
}

/// The text with `//` and `/* */` comments blanked (offsets and lines stay).
pub(crate) fn blank_comments(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0;
    let mut quote: Option<u8> = None;
    while i < b.len() {
        let c = b[i];
        if let Some(q) = quote {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q || c == b'\n' {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' => quote = Some(c),
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    out[i] = b' ';
                    i += 1;
                }
                continue;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                while i < b.len() && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                    if b[i] != b'\n' {
                        out[i] = b' ';
                    }
                    i += 1;
                }
                let end = (i + 2).min(b.len());
                out[i..end].fill(b' ');
                i += 2;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    // Only ASCII bytes were replaced, so the text stays valid UTF-8.
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

fn gradle(text: &str) -> Vec<Declared> {
    let clean = blank_comments(text);
    let text = clean.as_str();
    let mut out = Vec::new();
    // `implementation("g:a:v")`, `implementation 'g:a:v'`, `api(platform("g:a:v"))`.
    let short = Regex::new(&format!(
        r#"\b({CONFIGURATIONS})\s*\(?\s*(?:(?:platform|enforcedPlatform|testFixtures)\s*\(\s*)?["']([\w.\-]+):([\w.\-]+)(?::([^"'\s:@]+))?[^"']*["']"#
    ))
    .expect("valid regex");
    for c in short.captures_iter(text) {
        let whole = c.get(0).expect("match");
        let v = c.get(4);
        out.push(Declared {
            configuration: c[1].to_string(),
            group: c[2].to_string(),
            artifact: c[3].to_string(),
            version: v.map(|m| m.as_str().to_string()).unwrap_or_default(),
            line: line_of(text, whole.start()),
            version_range: v.map(|m| (m.start(), m.end())),
            resolved: v.map(|m| m.as_str().to_string()).unwrap_or_default(),
        });
    }
    // `implementation group: 'g', name: 'a', version: 'v'` and the Kotlin named form.
    let named = Regex::new(&format!(
        r#"\b({CONFIGURATIONS})\s*\(?\s*group\s*[:=]\s*["']([\w.\-]+)["']\s*,\s*name\s*[:=]\s*["']([\w.\-]+)["'](?:\s*,\s*version\s*[:=]\s*["']([^"']+)["'])?"#
    ))
    .expect("valid regex");
    for c in named.captures_iter(text) {
        let whole = c.get(0).expect("match");
        let v = c.get(4);
        out.push(Declared {
            configuration: c[1].to_string(),
            group: c[2].to_string(),
            artifact: c[3].to_string(),
            version: v.map(|m| m.as_str().to_string()).unwrap_or_default(),
            line: line_of(text, whole.start()),
            version_range: v.map(|m| (m.start(), m.end())),
            resolved: v.map(|m| m.as_str().to_string()).unwrap_or_default(),
        });
    }
    out.sort_by_key(|d| d.line);
    out
}

/// The text between `<name>` and `</name>` inside `block` (starting at byte `base`).
fn element(block: &str, base: usize, name: &str) -> Option<(String, (usize, usize))> {
    let open = format!("<{name}>");
    let s = block.find(&open)? + open.len();
    let e = block[s..].find('<')? + s;
    let raw = &block[s..e];
    let lead = raw.len() - raw.trim_start().len();
    let v = raw.trim();
    Some((v.to_string(), (base + s + lead, base + s + lead + v.len())))
}

fn pom(text: &str) -> Vec<Declared> {
    // Comments must not count; keep offsets by blanking them.
    let mut clean: Vec<u8> = text.as_bytes().to_vec();
    let mut i = 0;
    while let Some(s) = text[i..].find("<!--") {
        let s = i + s;
        let e = text[s..].find("-->").map_or(text.len(), |e| s + e + 3);
        for b in clean.iter_mut().take(e).skip(s) {
            if *b != b'\n' {
                *b = b' ';
            }
        }
        i = e;
        if i >= text.len() {
            break;
        }
    }
    let clean = String::from_utf8(clean).unwrap_or_else(|_| text.to_string());
    let mut properties: HashMap<String, (String, (usize, usize))> = HashMap::new();
    if let (Some(s), Some(e)) = (clean.find("<properties>"), clean.find("</properties>")) {
        if e > s {
            let body = &clean[s..e];
            if let Ok(re) = Regex::new(r"<([\w.\-]+)>\s*([^<]*?)\s*</([\w.\-]+)>") {
                for c in re.captures_iter(body) {
                    if c[1] == c[3] {
                        let m = c.get(2).expect("group");
                        properties.insert(
                            c[1].to_string(),
                            (m.as_str().to_string(), (s + m.start(), s + m.end())),
                        );
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    let mut rest = 0;
    while let Some(s) = clean[rest..].find("<dependency>") {
        let start = rest + s;
        let Some(len) = clean[start..].find("</dependency>") else {
            break;
        };
        let block = &clean[start..start + len];
        rest = start + len;
        let (Some((group, _)), Some((artifact, _))) = (
            element(block, start, "groupId"),
            element(block, start, "artifactId"),
        ) else {
            continue;
        };
        let scope = element(block, start, "scope")
            .map(|s| s.0)
            .unwrap_or_else(|| "compile".into());
        let v = element(block, start, "version");
        let (version, mut range, mut resolved) = match &v {
            Some((v, r)) => (v.clone(), Some(*r), v.clone()),
            None => (String::new(), None, String::new()),
        };
        if let Some(name) = version.strip_prefix("${").and_then(|v| v.strip_suffix('}')) {
            match properties.get(name) {
                Some((value, r)) => {
                    resolved = value.clone();
                    range = Some(*r);
                }
                None => range = None,
            }
        }
        out.push(Declared {
            configuration: scope,
            group,
            artifact,
            version,
            line: line_of(text, start),
            version_range: range,
            resolved,
        });
    }
    out
}

/// The dependencies a build file declares.
pub fn declarations(text: &str, kind: FileKind) -> Vec<Declared> {
    match kind {
        FileKind::GradleKotlin | FileKind::GradleGroovy => gradle(text),
        FileKind::Pom => pom(text),
    }
}

/// `group:artifact` of the declarations with a version to compare.
pub fn versioned_coordinates(text: &str, kind: FileKind) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for d in declarations(text, kind) {
        let key = format!("{}:{}", d.group, d.artifact);
        if d.version_range.is_some() && !d.resolved.is_empty() && !out.contains(&key) {
            out.push(key);
        }
    }
    out
}

/// A position as the editor counts it (0-based line, UTF-16 column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: usize,
    pub character: usize,
}

/// "A newer version is available" on a declared version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutdatedMarker {
    pub group: String,
    pub artifact: String,
    pub current: String,
    pub latest: String,
    /// UTF-16 range of the version text the quick fix replaces.
    pub from: usize,
    pub to: usize,
    /// 0-based line of the declaration (gutter icon).
    pub line: usize,
    pub start: Position,
    pub end: Position,
    pub message: String,
}

fn position(text: &str, u: &Utf16, byte: usize) -> Position {
    let line_start = text[..byte].rfind('\n').map_or(0, |i| i + 1);
    Position {
        line: line_of(text, byte),
        character: u.of(byte) - u.of(line_start),
    }
}

/// Markers of declarations older than the newest release of their variant (`versions`:
/// `group:artifact` → its versions).
pub fn outdated(
    text: &str,
    kind: FileKind,
    versions: &HashMap<String, Vec<String>>,
) -> Vec<OutdatedMarker> {
    let u = Utf16::new(text);
    let mut out = Vec::new();
    for d in declarations(text, kind) {
        let Some((from, to)) = d.version_range else {
            continue;
        };
        let Some(list) = versions.get(&format!("{}:{}", d.group, d.artifact)) else {
            continue;
        };
        let Some(newest) = version::update_for(&d.resolved, list) else {
            continue;
        };
        out.push(OutdatedMarker {
            message: format!(
                "Newer version of {}:{} is available: {newest}",
                d.group, d.artifact
            ),
            group: d.group,
            artifact: d.artifact,
            current: d.resolved,
            latest: newest,
            from: u.of(from),
            to: u.of(to),
            line: d.line,
            start: position(text, &u, from),
            end: position(text, &u, to),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const KTS: &str = r#"dependencies {
    implementation("com.google.guava:guava:32.0.0-jre")
    api(platform("org.junit:junit-bom:5.10.0"))
    testImplementation("org.junit.jupiter:junit-jupiter")
    implementation(group = "org.slf4j", name = "slf4j-api", version = "2.0.9")
    // implementation("commented:out:1.0")
}"#;

    #[test]
    fn gradle_declarations() {
        let d = declarations(KTS, FileKind::GradleKotlin);
        let ids: Vec<String> = d
            .iter()
            .map(|d| {
                format!(
                    "{}:{}:{}:{}",
                    d.configuration, d.group, d.artifact, d.version
                )
            })
            .collect();
        assert_eq!(
            ids,
            [
                "implementation:com.google.guava:guava:32.0.0-jre",
                "api:org.junit:junit-bom:5.10.0",
                "testImplementation:org.junit.jupiter:junit-jupiter:",
                "implementation:org.slf4j:slf4j-api:2.0.9",
            ]
        );
        assert_eq!(d[0].line, 1);
        let (s, e) = d[0].version_range.unwrap();
        assert_eq!(&KTS[s..e], "32.0.0-jre");
        let groovy = "dependencies {\n  testImplementation 'junit:junit:4.12'\n  implementation group: 'com.h2database', name: 'h2', version: '2.2.220'\n}";
        let d = declarations(groovy, FileKind::GradleGroovy);
        assert_eq!(d.len(), 2);
        assert_eq!(d[1].artifact, "h2");
        assert_eq!(
            FileKind::of_path("app/build.gradle.kts"),
            Some(FileKind::GradleKotlin)
        );
        assert_eq!(FileKind::of_path("pom.xml"), Some(FileKind::Pom));
        assert_eq!(FileKind::of_path("x.kt"), None);
    }

    const POM: &str = r#"<project>
  <properties>
    <guava.version>32.0.0-jre</guava.version>
  </properties>
  <dependencies>
    <dependency>
      <groupId>com.google.guava</groupId>
      <artifactId>guava</artifactId>
      <version>${guava.version}</version>
    </dependency>
    <!-- <dependency><groupId>x</groupId><artifactId>y</artifactId><version>1</version></dependency> -->
    <dependency>
      <groupId>org.junit.jupiter</groupId>
      <artifactId>junit-jupiter</artifactId>
      <version> 5.10.0 </version>
      <scope>test</scope>
    </dependency>
    <dependency><groupId>org.slf4j</groupId><artifactId>slf4j-api</artifactId></dependency>
  </dependencies>
</project>"#;

    #[test]
    fn pom_declarations_with_properties() {
        let d = declarations(POM, FileKind::Pom);
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].version, "${guava.version}");
        assert_eq!(d[0].resolved, "32.0.0-jre");
        let (s, e) = d[0].version_range.unwrap();
        assert_eq!(&POM[s..e], "32.0.0-jre");
        assert_eq!(d[1].configuration, "test");
        let (s, e) = d[1].version_range.unwrap();
        assert_eq!(&POM[s..e], "5.10.0");
        assert_eq!(d[2].version_range, None);
        assert_eq!(
            versioned_coordinates(POM, FileKind::Pom),
            ["com.google.guava:guava", "org.junit.jupiter:junit-jupiter"]
        );
    }

    #[test]
    fn outdated_markers() {
        let latest: HashMap<String, Vec<String>> = [
            (
                "com.google.guava:guava",
                vec!["33.3.1-jre", "33.3.1-android", "32.0.0-jre"],
            ),
            ("org.junit:junit-bom", vec!["5.10.0"]),
            ("org.slf4j:slf4j-api", vec!["2.0.9", "2.1.0-alpha1"]),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.into_iter().map(String::from).collect()))
        .collect();
        let m = outdated(KTS, FileKind::GradleKotlin, &latest);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].latest, "33.3.1-jre");
        assert_eq!(
            m[0].start,
            Position {
                line: 1,
                character: 43
            }
        );
        assert_eq!(&KTS[m[0].from..m[0].to], "32.0.0-jre");
        assert!(m[0].message.contains("33.3.1-jre"));
        let m = outdated(POM, FileKind::Pom, &latest);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].start.line, 2);
    }
}
