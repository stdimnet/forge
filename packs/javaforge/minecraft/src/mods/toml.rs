//! Completion in `neoforge.mods.toml` (and Forge's `mods.toml`): the keys of the root table,
//! `[[mods]]`, `[[dependencies.<modid>]]`, `[[mixins]]`, `[[accessTransformers]]` and
//! `[features.<modid>]` with what each means, the values of the enumerated keys (`type`,
//! `ordering`, `side`, `displayTest`, `modLoader`, booleans) and the table headers. Keys a
//! table already has are not offered again. A line scanner, not a TOML parser.
//!
//! Offsets in and out are UTF-16 code units.

use serde::Serialize;

/// Whether `path` is a NeoForge / Forge mod metadata file.
pub fn is_mods_toml(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name == "neoforge.mods.toml" || name == "mods.toml"
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TomlItem {
    pub label: String,
    /// The inserted text.
    pub apply: String,
    pub detail: String,
    /// `key`, `value` or `table`.
    pub kind: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TomlCompletion {
    pub from: usize,
    pub to: usize,
    pub items: Vec<TomlItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Table {
    Root,
    Mods,
    Dependency,
    Mixins,
    AccessTransformers,
    Features,
    Other,
}

/// (key, description, values) per table.
type Key = (&'static str, &'static str, &'static [&'static str]);

const BOOL: &[&str] = &["true", "false"];

const ROOT: &[Key] = &[
    (
        "modLoader",
        "The language loader: javafml (Java mods) or lowcodefml",
        &["javafml", "lowcodefml"],
    ),
    (
        "loaderVersion",
        "Accepted versions of the language loader, a Maven range such as [1,)",
        &[],
    ),
    (
        "license",
        "Required. The license of the mod, e.g. MIT or All Rights Reserved",
        &[],
    ),
    (
        "showAsResourcePack",
        "Show the mod's resources as a separate resource pack",
        BOOL,
    ),
    (
        "showAsDataPack",
        "Show the mod's data as a separate data pack",
        BOOL,
    ),
    (
        "services",
        "Service interfaces the mod uses (ServiceLoader)",
        &[],
    ),
    (
        "issueTrackerURL",
        "Where to report problems with the mod",
        &[],
    ),
];

const MODS: &[Key] = &[
    ("modId", "Required. The mod id, [a-z][a-z0-9_]{1,63}", &[]),
    ("version", "Required. The mod version, usually ${mod_version}", &[]),
    ("displayName", "Required. The name shown in the mod list", &[]),
    ("description", "The description shown in the mod list", &[]),
    ("authors", "The authors shown in the mod list", &[]),
    ("credits", "Credits shown in the mod list", &[]),
    ("logoFile", "An image in the jar's root shown in the mod list", &[]),
    ("logoBlur", "Blur the logo when it is scaled", BOOL),
    ("displayURL", "The mod's homepage, linked in the mod list", &[]),
    ("modUrl", "The mod's download page", &[]),
    ("updateJSONURL", "The update checker's JSON file", &[]),
    ("namespace", "The resource namespace, the mod id by default", &[]),
    ("enumExtensions", "A JSON file of enum extensions", &[]),
    ("featureFlags", "A JSON file of feature flags", &[]),
    (
        "displayTest",
        "Whether the mod must be on both sides: MATCH_VERSION (default), IGNORE_SERVER_VERSION (server-side mods), IGNORE_ALL_VERSION (client-side mods), NONE",
        &["MATCH_VERSION", "IGNORE_SERVER_VERSION", "IGNORE_ALL_VERSION", "NONE"],
    ),
];

const DEPENDENCY: &[Key] = &[
    ("modId", "Required. The mod this one depends on", &[]),
    (
        "type",
        "Required. required, optional, incompatible or discouraged",
        &["required", "optional", "incompatible", "discouraged"],
    ),
    (
        "versionRange",
        "Accepted versions, a Maven range such as [1.0,2.0)",
        &[],
    ),
    (
        "reason",
        "Why the dependency is needed or incompatible",
        &[],
    ),
    (
        "ordering",
        "Load BEFORE or AFTER the dependency, or NONE",
        &["NONE", "BEFORE", "AFTER"],
    ),
    (
        "side",
        "BOTH, CLIENT or SERVER",
        &["BOTH", "CLIENT", "SERVER"],
    ),
    ("referralUrl", "Where to get the dependency", &[]),
];

const MIXINS: &[Key] = &[
    (
        "config",
        "A mixin configuration in the resources, e.g. ${mod_id}.mixins.json",
        &[],
    ),
    (
        "requiredMods",
        "Apply the configuration only when these mods are present",
        &[],
    ),
    ("behaviorVersion", "The Mixin behavior version", &[]),
];

const ACCESS_TRANSFORMERS: &[Key] = &[(
    "file",
    "An access transformer in the resources, e.g. META-INF/accesstransformer.cfg",
    &[],
)];

const FEATURES: &[Key] = &[(
    "openGLVersion",
    "Required OpenGL versions, e.g. [3.2,)",
    &[],
)];

fn keys(table: Table) -> &'static [Key] {
    match table {
        Table::Root => ROOT,
        Table::Mods => MODS,
        Table::Dependency => DEPENDENCY,
        Table::Mixins => MIXINS,
        Table::AccessTransformers => ACCESS_TRANSFORMERS,
        Table::Features => FEATURES,
        Table::Other => &[],
    }
}

fn table_of(header: &str) -> Table {
    let h = header.trim();
    let inner = h.trim_start_matches('[');
    let inner = inner[..inner.find(']').unwrap_or(inner.len())].trim();
    if h.starts_with("[[") {
        match inner {
            "mods" => Table::Mods,
            "mixins" => Table::Mixins,
            "accessTransformers" => Table::AccessTransformers,
            i if i.starts_with("dependencies.") => Table::Dependency,
            _ => Table::Other,
        }
    } else if inner.starts_with("features.") {
        Table::Features
    } else {
        Table::Other
    }
}

fn utf16(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())]
        .chars()
        .map(char::len_utf16)
        .sum()
}

fn byte_of(text: &str, utf16_pos: usize) -> usize {
    let mut units = 0;
    for (i, c) in text.char_indices() {
        if units >= utf16_pos {
            return i;
        }
        units += c.len_utf16();
    }
    text.len()
}

/// The `modId` of the first `[[mods]]` entry, else `${mod_id}`.
fn mod_id(text: &str) -> String {
    let mut in_mods = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_mods = table_of(t) == Table::Mods;
        } else if in_mods {
            if let Some(v) = t.strip_prefix("modId") {
                let v = v.trim_start().trim_start_matches('=').trim();
                let v = v.split('#').next().unwrap_or("").trim().trim_matches('"');
                if !v.is_empty() {
                    return v.to_string();
                }
            }
        }
    }
    "${mod_id}".into()
}

/// Complete at `pos` (UTF-16) in `text`.
pub fn complete(text: &str, pos: usize) -> Option<TomlCompletion> {
    let at = byte_of(text, pos);
    let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let before = &text[line_start..at];
    let line_end = text[at..].find('\n').map_or(text.len(), |i| at + i);

    // The table the line is in, and the keys it already has (until the next header).
    let mut table = Table::Root;
    let mut block_start = 0;
    let mut offset = 0;
    for line in text[..line_start].split_inclusive('\n') {
        if line.trim_start().starts_with('[') {
            table = table_of(line);
            block_start = offset + line.len();
        }
        offset += line.len();
    }
    let block_end = text[line_end..]
        .split_inclusive('\n')
        .scan(line_end, |o, l| {
            let start = *o;
            *o += l.len();
            Some((start, l))
        })
        .find(|(_, l)| l.trim_start().starts_with('['))
        .map_or(text.len(), |(s, _)| s);
    let present: Vec<&str> = text[block_start..block_end]
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, _)| k.trim()))
        .collect();

    let trimmed = before.trim_start();
    let indent = before.len() - trimmed.len();

    // A table header.
    if trimmed.starts_with('[') {
        let id = mod_id(text);
        let headers = [
            ("[[mods]]", "A mod of this jar"),
            ("[[dependencies.ID]]", "A dependency of a mod"),
            ("[[mixins]]", "A mixin configuration to load"),
            ("[[accessTransformers]]", "An access transformer to apply"),
            ("[features.ID]", "Game environment features a mod requires"),
            ("[modproperties.ID]", "Custom properties of a mod"),
        ];
        let items = headers
            .iter()
            .map(|(h, d)| {
                let h = h.replace("ID", &id);
                TomlItem {
                    label: h.clone(),
                    apply: h,
                    detail: d.to_string(),
                    kind: "table",
                }
            })
            .collect();
        let from = line_start + indent;
        let to = line_end.min(from + text[from..line_end].trim_end().len());
        return Some(TomlCompletion {
            from: utf16(text, from),
            to: utf16(text, to.max(at)),
            items,
        });
    }

    // A value: `key = "par|`.
    if let Some((key, value)) = trimmed.split_once('=') {
        let key = key.trim();
        let value = value.trim_start();
        let quoted = value.starts_with('"');
        let partial = value.trim_start_matches('"');
        if partial.contains('"') || partial.contains('#') {
            return None;
        }
        let (_, _, values) = keys(table).iter().find(|(k, _, _)| *k == key)?;
        if values.is_empty() {
            return None;
        }
        let is_bool = *values == BOOL;
        let from = at - partial.len();
        let items = values
            .iter()
            .map(|v| TomlItem {
                label: v.to_string(),
                apply: if quoted || is_bool {
                    v.to_string()
                } else {
                    format!("\"{v}\"")
                },
                detail: String::new(),
                kind: "value",
            })
            .collect();
        return Some(TomlCompletion {
            from: utf16(text, from),
            to: pos,
            items,
        });
    }

    // A key at the start of the line.
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    let items: Vec<TomlItem> = keys(table)
        .iter()
        .filter(|(k, _, _)| !present.contains(k))
        .map(|(k, d, values)| TomlItem {
            label: k.to_string(),
            apply: if *values == BOOL {
                format!("{k} = ")
            } else {
                format!("{k} = \"\"")
            },
            detail: d.to_string(),
            kind: "key",
        })
        .collect();
    if items.is_empty() {
        return None;
    }
    Some(TomlCompletion {
        from: utf16(text, at - trimmed.len()),
        to: pos,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOML: &str = "modLoader = \"javafml\"\nlicense = \"MIT\"\n\n[[mods]]\nmodId = \"demo\"\nversion = \"1\"\n\n[[dependencies.demo]]\nmodId = \"neoforge\"\n";

    fn labels(c: &TomlCompletion) -> Vec<&str> {
        c.items.iter().map(|i| i.label.as_str()).collect()
    }

    #[test]
    fn completes_keys_of_the_current_table() {
        // At the end of the [[mods]] block (the empty line before the dependency).
        let pos = TOML.find("\n\n[[dep").unwrap() + 1;
        let c = complete(TOML, pos).unwrap();
        let l = labels(&c);
        assert!(l.contains(&"displayName"));
        assert!(!l.contains(&"modId"), "{l:?}");
        assert!(!l.contains(&"license"));
        // Root keys at the top.
        let text = format!("lo\n{TOML}");
        let c = complete(&text, 2).unwrap();
        assert_eq!(c.from, 0);
        assert!(labels(&c).contains(&"loaderVersion"));
        assert!(!labels(&c).contains(&"modLoader"));
        assert_eq!(c.items[0].apply, "loaderVersion = \"\"");
        // Dependencies.
        let text = format!("{TOML}ty");
        let c = complete(&text, text.len()).unwrap();
        assert!(labels(&c).contains(&"type"));
        assert!(!labels(&c).contains(&"modId"));
    }

    #[test]
    fn completes_values() {
        let text = format!("{TOML}side = \"");
        let c = complete(&text, text.len()).unwrap();
        assert_eq!(labels(&c), ["BOTH", "CLIENT", "SERVER"]);
        assert_eq!(c.items[0].apply, "BOTH");
        let text = format!("{TOML}ordering = ");
        let c = complete(&text, text.len()).unwrap();
        assert_eq!(c.items[0].apply, "\"NONE\"");
        let text = "[[mods]]\nlogoBlur = t";
        let c = complete(text, text.len()).unwrap();
        assert_eq!(c.from, text.len() - 1);
        assert_eq!(c.items[0].apply, "true");
        assert!(complete("[[mods]]\nmodId = \"x", 18).is_none());
    }

    #[test]
    fn completes_headers_with_the_mod_id() {
        let text = format!("{TOML}\n[[");
        let c = complete(&text, text.len()).unwrap();
        assert!(labels(&c).contains(&"[[dependencies.demo]]"));
        assert!(labels(&c).contains(&"[[mixins]]"));
        assert_eq!(utf16(&text, text.len() - 2), c.from);
        let c = complete("[", 1).unwrap();
        assert!(labels(&c).contains(&"[features.${mod_id}]"));
        assert!(is_mods_toml(
            "src/main/templates/META-INF/neoforge.mods.toml"
        ));
        assert!(!is_mods_toml("Cargo.toml"));
    }
}
