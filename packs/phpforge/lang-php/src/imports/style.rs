//! How `use` statements are arranged and when classes are imported: the PHP code style's
//! Imports tab (`php.codeStyle.imports.*`: sorting, grouping, blank lines between groups,
//! "Prefer FQN") and the PHP part of Settings | Editor | General | Auto Import
//! (`php.autoImport.*`).
//!
//! Every place that adds or rewrites imports reads one [`ImportStyle`]: Optimize Imports,
//! Actions on Save, Auto Import (completion, on the fly, Import class) and the refactorings
//! (Move, Copy, Pull Members Up, Create from usage). The refactorings deep inside the crate
//! read the style of the current call through [`with_style`] / [`current`], so the app sets
//! it once around a call instead of threading it through every function.

use std::cell::Cell;
use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::UseKind;

pub const SORT_KEY: &str = "php.codeStyle.imports.sort";
pub const GROUP_BY_KEY: &str = "php.codeStyle.imports.groupBy";
pub const BLANK_LINES_KEY: &str = "php.codeStyle.imports.blankLinesBetweenGroups";
pub const PREFER_FQN_KEY: &str = "php.codeStyle.imports.preferFqn";
pub const NAMESPACE_SCOPE_KEY: &str = "php.autoImport.namespaceScope";
pub const FILE_SCOPE_KEY: &str = "php.autoImport.fileScope";
pub const GLOBAL_NAMESPACE_KEY: &str = "php.autoImport.globalNamespace";
pub const FQN_IN_PHPDOC_KEY: &str = "php.autoImport.fqnInPhpDoc";

/// Order of the `use` statements inside a group.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportSort {
    /// By name, ignoring case (PHPStorm's default).
    #[default]
    Alphabetic,
    /// Shorter statements first, then by name.
    ByLength,
    /// Keep the order; new imports go to the end of their group.
    None,
}

/// How `use` statements are split into groups.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportGrouping {
    /// Classes, then functions, then constants.
    #[default]
    Kind,
    /// By kind, and inside a kind by the top-level namespace (`App`, `Illuminate`).
    Namespace,
    /// One group.
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImportStyle {
    pub sort: ImportSort,
    pub group_by: ImportGrouping,
    pub blank_lines_between_groups: usize,
    /// Insert `\App\Models\User` instead of importing it.
    pub prefer_fqn: bool,
    /// Auto-import in files with a namespace (else the fully qualified name is inserted).
    pub namespace_scope: bool,
    /// Auto-import in files without a namespace.
    pub file_scope: bool,
    /// Import classes of the global namespace (`use Exception;`) instead of writing
    /// `\Exception` in namespaced files.
    pub import_global_classes: bool,
    /// PHPDoc comments get fully qualified names instead of imports.
    pub fqn_in_phpdoc: bool,
}

impl Default for ImportStyle {
    fn default() -> Self {
        Self {
            sort: ImportSort::Alphabetic,
            group_by: ImportGrouping::Kind,
            blank_lines_between_groups: 0,
            prefer_fqn: false,
            namespace_scope: true,
            file_scope: false,
            import_global_classes: true,
            fqn_in_phpdoc: false,
        }
    }
}

impl ImportStyle {
    /// The style from Forge's settings (`get` answers the effective value of a key, with
    /// the selected code style scheme applied).
    pub fn from_settings(get: impl Fn(&str) -> Option<Value>) -> Self {
        let d = Self::default();
        let flag = |key: &str, default: bool| get(key).and_then(|v| v.as_bool()).unwrap_or(default);
        let text = |key: &str| get(key).and_then(|v| v.as_str().map(str::to_string));
        Self {
            sort: match text(SORT_KEY).as_deref() {
                Some("byLength") => ImportSort::ByLength,
                Some("none") => ImportSort::None,
                _ => ImportSort::Alphabetic,
            },
            group_by: match text(GROUP_BY_KEY).as_deref() {
                Some("namespace") => ImportGrouping::Namespace,
                Some("none") => ImportGrouping::None,
                _ => ImportGrouping::Kind,
            },
            blank_lines_between_groups: get(BLANK_LINES_KEY)
                .and_then(|v| v.as_u64())
                .map_or(d.blank_lines_between_groups, |n| n.min(3) as usize),
            prefer_fqn: flag(PREFER_FQN_KEY, d.prefer_fqn),
            namespace_scope: flag(NAMESPACE_SCOPE_KEY, d.namespace_scope),
            file_scope: flag(FILE_SCOPE_KEY, d.file_scope),
            import_global_classes: flag(GLOBAL_NAMESPACE_KEY, d.import_global_classes),
            fqn_in_phpdoc: flag(FQN_IN_PHPDOC_KEY, d.fqn_in_phpdoc),
        }
    }

    /// The group an import belongs to; groups are written in this order.
    pub fn group_of(&self, kind: UseKind, fqn: &str) -> (u8, String) {
        let rank = match kind {
            UseKind::Class => 0,
            UseKind::Function => 1,
            UseKind::Const => 2,
        };
        match self.group_by {
            ImportGrouping::None => (0, String::new()),
            ImportGrouping::Kind => (rank, String::new()),
            ImportGrouping::Namespace => {
                let fqn = fqn.trim_start_matches('\\');
                // Classes of the global namespace form their own group first.
                let top = match fqn.split_once('\\') {
                    Some((top, _)) => top.to_ascii_lowercase(),
                    None => String::new(),
                };
                (rank, top)
            }
        }
    }

    /// Order of two imports of one group (`rendered` is the whole statement, which "By
    /// length" compares). `Equal` with [`ImportSort::None`]: new imports go last.
    pub fn compare(
        &self,
        a_fqn: &str,
        a_rendered: &str,
        b_fqn: &str,
        b_rendered: &str,
    ) -> Ordering {
        let by_name = || a_fqn.to_lowercase().cmp(&b_fqn.to_lowercase());
        match self.sort {
            ImportSort::Alphabetic => by_name(),
            ImportSort::ByLength => a_rendered.len().cmp(&b_rendered.len()).then_with(by_name),
            ImportSort::None => Ordering::Equal,
        }
    }

    /// The text between two groups: the line break plus the blank lines.
    pub fn group_separator(&self) -> String {
        "\n".repeat(self.blank_lines_between_groups + 1)
    }
}

thread_local! {
    static CURRENT: Cell<Option<ImportStyle>> = const { Cell::new(None) };
}

/// Run `f` with `style` as the import style of every import the crate adds meanwhile
/// (on this thread).
pub fn with_style<T>(style: ImportStyle, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<ImportStyle>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CURRENT.with(|c| c.set(self.0));
        }
    }
    let _restore = Restore(CURRENT.with(|c| c.replace(Some(style))));
    f()
}

/// The import style of the running call ([`with_style`]), else the default one.
pub fn current() -> ImportStyle {
    CURRENT.with(|c| c.get()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn reads_code_style_and_auto_import_settings() {
        let values: HashMap<&str, Value> = [
            (SORT_KEY, json!("byLength")),
            (GROUP_BY_KEY, json!("namespace")),
            (BLANK_LINES_KEY, json!(9)),
            (PREFER_FQN_KEY, json!(true)),
            (FILE_SCOPE_KEY, json!(true)),
            (GLOBAL_NAMESPACE_KEY, json!(false)),
        ]
        .into_iter()
        .collect();
        let s = ImportStyle::from_settings(|k| values.get(k).cloned());
        assert_eq!(s.sort, ImportSort::ByLength);
        assert_eq!(s.group_by, ImportGrouping::Namespace);
        assert_eq!(s.blank_lines_between_groups, 3, "clamped");
        assert!(s.prefer_fqn && s.file_scope && s.namespace_scope);
        assert!(!s.import_global_classes && !s.fqn_in_phpdoc);
        assert_eq!(ImportStyle::from_settings(|_| None), ImportStyle::default());
    }

    #[test]
    fn groups_and_orders() {
        let mut s = ImportStyle::default();
        assert_eq!(
            s.group_of(UseKind::Class, "App\\A"),
            s.group_of(UseKind::Class, "Vendor\\B")
        );
        assert!(s.group_of(UseKind::Class, "Z") < s.group_of(UseKind::Function, "A\\f"));
        s.group_by = ImportGrouping::Namespace;
        assert!(s.group_of(UseKind::Class, "Exception") < s.group_of(UseKind::Class, "App\\A"));
        assert!(s.group_of(UseKind::Class, "App\\A") < s.group_of(UseKind::Class, "Illuminate\\B"));
        s.group_by = ImportGrouping::None;
        assert_eq!(
            s.group_of(UseKind::Const, "A"),
            s.group_of(UseKind::Class, "B")
        );

        assert_eq!(s.compare("a\\B", "", "A\\c", ""), Ordering::Less);
        s.sort = ImportSort::ByLength;
        assert_eq!(
            s.compare("Zz", "use Zz;", "Aaa", "use Aaa;"),
            Ordering::Less
        );
        s.sort = ImportSort::None;
        assert_eq!(s.compare("Z", "", "A", ""), Ordering::Equal);
    }

    #[test]
    fn scoped_style_is_restored() {
        let mine = ImportStyle {
            prefer_fqn: true,
            ..Default::default()
        };
        assert!(!current().prefer_fqn);
        let inner = with_style(mine, || {
            assert!(current().prefer_fqn);
            with_style(ImportStyle::default(), || assert!(!current().prefer_fqn));
            current().prefer_fqn
        });
        assert!(inner);
        assert!(!current().prefer_fqn);
    }
}
