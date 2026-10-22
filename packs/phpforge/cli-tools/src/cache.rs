//! The per-tool command cache (`<cache dir>/forge/command-line-tools/<key>.json`): a tool's
//! commands stay valid while its fingerprint (kind, path, interpreter and the text of
//! `composer.lock`) is unchanged, so installing or updating packages refreshes them.

use serde::{Deserialize, Serialize};

use crate::definitions::CommandDef;
use crate::model::Tool;

/// Bumped when the cached format changes.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheEntry {
    pub version: u32,
    pub fingerprint: String,
    pub commands: Vec<CommandDef>,
}

/// 64-bit FNV-1a: stable across Rust versions and platforms (unlike `DefaultHasher`).
pub fn fnv1a(parts: &[&str]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            h ^= 0x1f;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        for b in part.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// What the cached commands depend on.
pub fn fingerprint(tool: &Tool, composer_lock: Option<&str>) -> String {
    format!(
        "{:016x}",
        fnv1a(&[
            tool.kind.id(),
            tool.path.trim(),
            tool.interpreter.trim(),
            composer_lock.unwrap_or(""),
        ])
    )
}

/// The cache file of a tool in a project.
pub fn file_name(project_root: &str, tool: &Tool) -> String {
    format!(
        "{}-{:016x}.json",
        tool.kind.id(),
        fnv1a(&[project_root, &tool.alias])
    )
}

/// The cached commands when `text` is a valid entry for `fingerprint`.
pub fn read(text: &str, fingerprint: &str) -> Option<Vec<CommandDef>> {
    let entry: CacheEntry = serde_json::from_str(text).ok()?;
    (entry.version == FORMAT_VERSION && entry.fingerprint == fingerprint).then_some(entry.commands)
}

pub fn write(fingerprint: &str, commands: &[CommandDef]) -> String {
    serde_json::to_string(&CacheEntry {
        version: FORMAT_VERSION,
        fingerprint: fingerprint.into(),
        commands: commands.to_vec(),
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ToolKind;

    #[test]
    fn lock_changes_invalidate_the_cache() {
        let tool = Tool::new(ToolKind::Artisan);
        let a = fingerprint(&tool, Some("{\"packages\":[]}"));
        let b = fingerprint(&tool, Some("{\"packages\":[{}]}"));
        assert_ne!(a, b);
        assert_eq!(a, fingerprint(&tool, Some("{\"packages\":[]}")));
        let cmds = vec![CommandDef {
            name: "about".into(),
            ..Default::default()
        }];
        let text = write(&a, &cmds);
        assert_eq!(read(&text, &a), Some(cmds));
        assert_eq!(read(&text, &b), None);
        assert_eq!(read("garbage", &a), None);
        // Separator between parts: ("ab", "c") and ("a", "bc") differ.
        assert_ne!(fnv1a(&["ab", "c"]), fnv1a(&["a", "bc"]));
        assert!(file_name("/p", &tool).starts_with("artisan-"));
        assert_ne!(file_name("/p", &tool), file_name("/q", &tool));
    }
}
