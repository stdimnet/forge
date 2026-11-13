//! `AndroidManifest.xml`: the package, the activities and the launcher activity (the one
//! with the `MAIN` action and the `LAUNCHER` category), read with a small tolerant XML
//! scanner. Android Studio's "Android App" configuration launches that activity; it reads
//! the merged manifest of the last build when there is one, else the source manifest.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    /// Fully qualified class name.
    pub name: String,
    pub exported: Option<bool>,
    pub launcher: bool,
    /// An `<activity-alias>` (its `targetActivity` is `target`).
    pub alias: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// The `package` attribute (merged manifests always have it; AGP 8 source manifests
    /// use the Gradle `namespace` instead).
    pub package: Option<String>,
    pub activities: Vec<Activity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_sdk: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl Manifest {
    /// The activity a launch starts: the first launcher activity (an alias launches its own
    /// name, which `am start` resolves).
    pub fn launcher_activity(&self) -> Option<&Activity> {
        self.activities.iter().find(|a| a.launcher)
    }
}

#[derive(Debug)]
struct Tag {
    name: String,
    attrs: Vec<(String, String)>,
    closing: bool,
    self_closing: bool,
}

impl Tag {
    fn attr(&self, name: &str) -> Option<&str> {
        // `android:name`, or any prefix bound to the Android namespace.
        self.attrs
            .iter()
            .find(|(k, _)| k == name || k.rsplit(':').next() == Some(name))
            .map(|(_, v)| v.as_str())
    }
}

fn tags(xml: &str) -> Vec<Tag> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        if rest.starts_with("!--") {
            match rest.find("-->") {
                Some(end) => rest = &rest[end + 3..],
                None => break,
            }
            continue;
        }
        if rest.starts_with('?') || rest.starts_with('!') {
            if let Some(end) = rest.find('>') {
                rest = &rest[end + 1..];
            }
            continue;
        }
        // The tag's end, outside of quoted attribute values.
        let mut end = None;
        let mut quote: Option<char> = None;
        for (i, c) in rest.char_indices() {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (None, '"') | (None, '\'') => quote = Some(c),
                (None, '>') => {
                    end = Some(i);
                    break;
                }
                _ => {}
            }
        }
        let Some(end) = end else { break };
        let body = &rest[..end];
        rest = &rest[end + 1..];
        let closing = body.starts_with('/');
        let self_closing = body.ends_with('/');
        let body = body.trim_start_matches('/').trim_end_matches('/');
        let name_end = body.find(char::is_whitespace).unwrap_or(body.len());
        let name = body[..name_end].to_string();
        let mut attrs = Vec::new();
        let mut a = &body[name_end..];
        while let Some(eq) = a.find('=') {
            let key = a[..eq].trim().to_string();
            let after = a[eq + 1..].trim_start();
            let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                break;
            };
            let Some(close) = after[1..].find(q) else {
                break;
            };
            attrs.push((key, unescape(&after[1..1 + close])));
            a = &after[close + 2..];
        }
        out.push(Tag {
            name,
            attrs,
            closing,
            self_closing,
        });
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// A class name of the manifest, qualified with the package (`.MainActivity`,
/// `MainActivity` → `com.acme.notes.MainActivity`).
pub fn qualify(name: &str, package: Option<&str>) -> String {
    match package {
        Some(p) if name.starts_with('.') => format!("{p}{name}"),
        Some(p) if !name.contains('.') => format!("{p}.{name}"),
        _ => name.to_string(),
    }
}

/// Read a manifest. `namespace` (the module's Gradle `namespace`) qualifies relative class
/// names when the manifest has no `package`.
pub fn parse(xml: &str, namespace: Option<&str>) -> Manifest {
    let mut m = Manifest::default();
    let mut current: Option<Activity> = None;
    let mut in_filter = false;
    let mut main_action = false;
    let mut launcher_category = false;
    for t in tags(xml) {
        match (t.name.as_str(), t.closing) {
            ("manifest", false) => m.package = t.attr("package").map(str::to_string),
            ("uses-sdk", false) => m.min_sdk = t.attr("minSdkVersion").map(str::to_string),
            ("application", false) => m.label = t.attr("label").map(str::to_string),
            ("activity" | "activity-alias", false) => {
                let package = m.package.clone().or(namespace.map(str::to_string));
                let alias = t.name == "activity-alias";
                let a = Activity {
                    name: qualify(t.attr("name").unwrap_or_default(), package.as_deref()),
                    exported: t.attr("exported").map(|e| e == "true"),
                    launcher: false,
                    alias,
                    target: alias
                        .then(|| {
                            t.attr("targetActivity")
                                .map(|n| qualify(n, package.as_deref()))
                        })
                        .flatten(),
                };
                if t.self_closing {
                    m.activities.push(a);
                } else {
                    current = Some(a);
                }
            }
            ("activity" | "activity-alias", true) => {
                if let Some(a) = current.take() {
                    m.activities.push(a);
                }
            }
            ("intent-filter", false) => {
                in_filter = true;
                main_action = false;
                launcher_category = false;
            }
            ("intent-filter", true) => {
                if main_action && launcher_category {
                    if let Some(a) = current.as_mut() {
                        a.launcher = true;
                    }
                }
                in_filter = false;
            }
            ("action", false) if in_filter => {
                main_action |= t.attr("name") == Some("android.intent.action.MAIN");
            }
            ("category", false) if in_filter => {
                launcher_category |= matches!(
                    t.attr("name"),
                    Some("android.intent.category.LAUNCHER")
                        | Some("android.intent.category.LEANBACK_LAUNCHER")
                );
            }
            _ => {}
        }
    }
    m
}

/// Where AGP writes the merged manifest of a variant, newest layout first (relative to the
/// module folder).
pub fn merged_manifest_candidates(variant: &str) -> Vec<String> {
    let cap = capitalize(variant);
    vec![
        format!("build/intermediates/merged_manifests/{variant}/process{cap}Manifest/AndroidManifest.xml"),
        format!("build/intermediates/merged_manifest/{variant}/process{cap}MainManifest/AndroidManifest.xml"),
        format!("build/intermediates/merged_manifest/{variant}/AndroidManifest.xml"),
        format!("build/intermediates/merged_manifests/{variant}/AndroidManifest.xml"),
    ]
}

/// `debug` → `Debug`, `freeDebug` → `FreeDebug`.
pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_manifest_with_namespace() {
        let m = parse(
            include_str!("../tests/fixtures/AndroidManifest.xml"),
            Some("com.acme.notes"),
        );
        assert_eq!(m.package, None);
        let names: Vec<(&str, bool)> = m
            .activities
            .iter()
            .map(|a| (a.name.as_str(), a.launcher))
            .collect();
        assert_eq!(
            names,
            [
                ("com.acme.notes.MainActivity", true),
                ("com.acme.notes.settings.SettingsActivity", false),
                ("com.acme.notes.ShareActivity", false),
            ]
        );
        assert_eq!(m.launcher_activity().unwrap().exported, Some(true));
        assert_eq!(m.label.as_deref(), Some("@string/app_name"));
    }

    #[test]
    fn merged_manifest_and_aliases() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="com.acme.notes.debug">
    <uses-sdk android:minSdkVersion="26" android:targetSdkVersion="35" />
    <application android:label="Notes">
        <activity android:name="com.acme.notes.MainActivity" android:exported="false"/>
        <activity-alias android:name=".Launcher" android:targetActivity="com.acme.notes.MainActivity" android:exported="true">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity-alias>
    </application>
</manifest>"#;
        let m = parse(xml, None);
        assert_eq!(m.package.as_deref(), Some("com.acme.notes.debug"));
        assert_eq!(m.min_sdk.as_deref(), Some("26"));
        let l = m.launcher_activity().unwrap();
        assert_eq!(l.name, "com.acme.notes.debug.Launcher");
        assert!(l.alias);
        assert_eq!(l.target.as_deref(), Some("com.acme.notes.MainActivity"));
        assert_eq!(qualify("Main", Some("a.b")), "a.b.Main");
        assert_eq!(
            merged_manifest_candidates("freeDebug")[0],
            "build/intermediates/merged_manifests/freeDebug/processFreeDebugManifest/AndroidManifest.xml"
        );
    }
}
