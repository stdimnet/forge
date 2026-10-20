//! Window chrome for Forge's own (undecorated) windows.
//!
//! - [`desktop`]: which OS and desktop environment Forge runs on, read from the target OS and
//!   environment variables (`XDG_CURRENT_DESKTOP`, `DESKTOP_SESSION`, …).
//! - [`chrome`]: the title bar style and button layout for the setting
//!   `appearance.windowControls` (`auto | windows | macos | gnome | kde | linux | native`),
//!   including GNOME's `button-layout` and KDE's `kwinrc` button order.
//! - [`geometry`]: where a dialog window opens (centered on its parent, or where the user left
//!   it) and the remembered size and position of every dialog.
//!
//! The crate is pure (no file system, processes or windowing): the Tauri shell reads the
//! environment and files and passes them in, and the browser-mode UI runs the same code as
//! WebAssembly.

pub mod chrome;
pub mod desktop;
pub mod geometry;

pub use chrome::{
    resolve, Button, ButtonLayout, ControlsStyle, SystemLayouts, TitleAlign, WindowChrome,
};
pub use desktop::{detect_desktop, Desktop, Os};
pub use geometry::{place_dialog, DialogStates, Rect, Size};

/// Label prefix of dialog windows (`dialog-settings`); the Tauri capabilities match it.
pub const DIALOG_LABEL_PREFIX: &str = "dialog-";

/// Window label for a dialog id, or `None` when the id is not a safe label
/// (letters, digits, `-` and `_`, at most 64 characters).
pub fn dialog_label(id: &str) -> Option<String> {
    let valid = !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valid.then(|| format!("{DIALOG_LABEL_PREFIX}{id}"))
}

/// Dialog id of a dialog window label.
pub fn dialog_id(label: &str) -> Option<&str> {
    label
        .strip_prefix(DIALOG_LABEL_PREFIX)
        .filter(|id| !id.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_round_trip() {
        assert_eq!(dialog_label("settings").as_deref(), Some("dialog-settings"));
        assert_eq!(
            dialog_label("run-configurations").as_deref(),
            Some("dialog-run-configurations")
        );
        assert_eq!(dialog_id("dialog-settings"), Some("settings"));
        assert_eq!(dialog_id("main"), None);
        assert_eq!(dialog_id("dialog-"), None);
    }

    #[test]
    fn rejects_unsafe_ids() {
        assert_eq!(dialog_label(""), None);
        assert_eq!(dialog_label("../x"), None);
        assert_eq!(dialog_label("a b"), None);
        assert_eq!(dialog_label(&"x".repeat(65)), None);
    }
}
