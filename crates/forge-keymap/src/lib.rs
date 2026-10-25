//! Forge's PHPStorm-compatible keymap engine.
//!
//! - [`shortcut`]: parsing ("Ctrl+K Ctrl+C", "Shift Shift", "Cmd+,"), canonical ids, display
//!   strings ("Ctrl+Shift+A" / "⇧⌘A") and key names of DOM keyboard events.
//! - [`catalog`]: PHPStorm's default keymaps (Windows/Linux and macOS), the bundled keymaps
//!   derived from them (VS Code, Eclipse, Sublime Text, Emacs) and action aliases.
//! - [`engine`]: bindings, conflict detection and the [`Resolver`] state machine that turns
//!   key events into actions (chords, double taps, focus contexts).
//! - [`keymap`]: the [`KeymapService`] combining the catalog, plugin actions, registered
//!   commands, user overrides and user keymaps from the settings.
//! - [`menu`]: the main menu (PHPStorm's structure, plugin contributions, resolution).
//!
//! The crate is pure (no file system or async runtime): the desktop app wraps it in IPC
//! commands, and browser mode runs the same code as WebAssembly (`crates/forge-wasm`).

pub mod catalog;
pub mod engine;
pub mod keymap;
pub mod menu;
pub mod shortcut;

pub use catalog::{bundled_keymaps, catalog, ActionContext, ActionDef, BundledKeymap, KeymapId};
pub use engine::{FocusContext, KeyInput, Pending, Resolution, Resolver};
pub use keymap::{
    legacy_changes, ActionInfo, ActionRegistration, CommandInfo, KeymapChange, KeymapService,
    KeymapSettings, KeymapSnapshot, SettingWrite, ShortcutCheck,
};
pub use menu::{MainMenu, MainMenuRequest, MenuContribution, MenuEntry, MenuItem};
pub use shortcut::{KeyEvent, Platform, Shortcut};
