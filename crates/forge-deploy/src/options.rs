//! Deployment | Options (the `deployment.*` settings of the project).

use serde::{Deserialize, Serialize};

/// "Upload changed files automatically to the default server".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AutoUpload {
    /// Every save, also automatic saves (frame deactivation, closing tabs).
    Always,
    /// Only explicit saves (Ctrl+S, Save All).
    Explicit,
    #[default]
    Never,
}

impl AutoUpload {
    pub fn parse(value: &str) -> Self {
        match value {
            "always" => Self::Always,
            "explicit" => Self::Explicit,
            _ => Self::Never,
        }
    }

    /// Whether a save uploads (`explicit`: the user pressed Ctrl+S or chose Save All).
    pub fn uploads_on_save(self, explicit: bool) -> bool {
        match self {
            Self::Always => true,
            Self::Explicit => explicit,
            Self::Never => false,
        }
    }
}

/// "Warn when uploading over newer file".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WarnNewer {
    /// The target is newer and has another size.
    #[default]
    TimestampAndSize,
    /// The target is newer and has other content.
    Content,
    No,
}

impl WarnNewer {
    pub fn parse(value: &str) -> Self {
        match value {
            "content" => Self::Content,
            "no" => Self::No,
            _ => Self::TimestampAndSize,
        }
    }
}

/// How Sync with Deployed and the up-to-date check compare files.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompareBy {
    /// Size and content (reads both sides when the sizes are equal).
    #[default]
    Content,
    Size,
    /// Size and modification time.
    Timestamp,
}

impl CompareBy {
    pub fn parse(value: &str) -> Self {
        match value {
            "size" => Self::Size,
            "timestamp" => Self::Timestamp,
            _ => Self::Content,
        }
    }
}

/// The options that change how files are transferred.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeployOptions {
    pub auto_upload: AutoUpload,
    /// "Exclude items by name": `;`-separated names with `*` and `?`.
    pub exclude_names: String,
    /// "Overwrite up-to-date files": off skips files whose target has the same size and
    /// is not older.
    pub overwrite_up_to_date: bool,
    /// "Preserve files timestamps": the target gets the source's modification time.
    pub preserve_timestamps: bool,
    /// "Delete target items when source ones do not exist" (uploading or downloading
    /// folders, Sync).
    pub delete_target_items: bool,
    /// "Create empty directories".
    pub create_empty_dirs: bool,
    pub warn_newer: WarnNewer,
    pub compare_by: CompareBy,
}

/// PHPStorm's default "Exclude items by name", plus Forge's own `.forge` folder.
pub const DEFAULT_EXCLUDE_NAMES: &str = ".svn;.cvs;.idea;.DS_Store;.git;.hg;.forge";

impl Default for DeployOptions {
    fn default() -> Self {
        Self {
            auto_upload: AutoUpload::Never,
            exclude_names: DEFAULT_EXCLUDE_NAMES.to_string(),
            overwrite_up_to_date: false,
            preserve_timestamps: true,
            delete_target_items: false,
            create_empty_dirs: false,
            warn_newer: WarnNewer::TimestampAndSize,
            compare_by: CompareBy::Content,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_setting_values() {
        assert_eq!(AutoUpload::parse("always"), AutoUpload::Always);
        assert_eq!(AutoUpload::parse("explicit"), AutoUpload::Explicit);
        assert_eq!(AutoUpload::parse("bogus"), AutoUpload::Never);
        assert!(AutoUpload::Always.uploads_on_save(false));
        assert!(AutoUpload::Explicit.uploads_on_save(true));
        assert!(!AutoUpload::Explicit.uploads_on_save(false));
        assert!(!AutoUpload::Never.uploads_on_save(true));
        assert_eq!(WarnNewer::parse("content"), WarnNewer::Content);
        assert_eq!(WarnNewer::parse("no"), WarnNewer::No);
        assert_eq!(WarnNewer::parse(""), WarnNewer::TimestampAndSize);
        assert_eq!(CompareBy::parse("size"), CompareBy::Size);
        assert_eq!(CompareBy::parse("timestamp"), CompareBy::Timestamp);
        assert_eq!(CompareBy::parse("content"), CompareBy::Content);
        let json = serde_json::to_string(&DeployOptions::default()).unwrap();
        assert!(json.contains("\"autoUpload\":\"never\""), "{json}");
        assert!(
            json.contains("\"warnNewer\":\"timestampAndSize\""),
            "{json}"
        );
    }
}
