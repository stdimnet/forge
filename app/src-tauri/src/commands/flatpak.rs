//! Flatpak mode: whether Forge runs in a Flatpak sandbox and starts host tools through
//! `flatpak-spawn --host` (see `forge_hostspawn`).

use serde::Serialize;

/// What Settings | Tools | Flatpak shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlatpakStatus {
    pub in_flatpak: bool,
    pub flatpak_id: Option<String>,
    /// The setting "Run tools on the host (Flatpak)".
    pub enabled: bool,
    /// Host tools run through `flatpak-spawn --host` right now.
    pub active: bool,
    /// The host's system folders are visible under `/run/host`.
    pub host_os_visible: bool,
}

pub fn status() -> FlatpakStatus {
    let s = forge_process::host::status();
    FlatpakStatus {
        in_flatpak: s.in_flatpak,
        flatpak_id: s.flatpak_id,
        enabled: s.enabled,
        active: s.active,
        host_os_visible: s.host_os_visible,
    }
}

/// Apply the setting [`forge_process::host::SETTING_KEY`] (default on).
pub fn apply_setting(value: Option<bool>) {
    forge_process::host::set_enabled(value.unwrap_or(true));
}

#[tauri::command]
pub fn flatpak_status() -> FlatpakStatus {
    status()
}
