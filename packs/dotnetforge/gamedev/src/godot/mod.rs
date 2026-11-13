//! Godot (GDScript and C#), like Rider's Godot support:
//!
//! - [`project`]: `project.godot` (name, main scene, version and features, C#), scenes,
//! - [`executable`]: the Godot executable (Settings, `GODOT4` / `GODOT`, the `PATH`, the
//!   usual install folders) and its version,
//! - [`commands`]: the "Godot" run configuration (run the project, the current scene or a
//!   chosen one) and opening the editor,
//! - [`lsp`]: the editor's GDScript language server (TCP, port 6005),
//! - [`dap`]: the editor's debug adapter (TCP, port 6006) for GDScript; C# debugs through
//!   netcoredbg launching Godot (the app builds that request),
//! - [`gdunit`]: GdUnit4 test suites.

pub mod commands;
pub mod dap;
pub mod executable;
pub mod gdunit;
pub mod lsp;
pub mod project;

pub use project::{parse_project_godot, GodotProject};

/// `res://scenes/main.tscn` → `scenes/main.tscn`.
pub fn res_to_path(res: &str) -> String {
    res.trim()
        .strip_prefix("res://")
        .unwrap_or(res.trim())
        .trim_start_matches('/')
        .to_string()
}

/// `scenes/main.tscn` → `res://scenes/main.tscn` (already `res://` paths stay).
pub fn path_to_res(path: &str) -> String {
    let p = path.trim().replace('\\', "/");
    if p.starts_with("res://") {
        p
    } else {
        format!(
            "res://{}",
            p.trim_start_matches("./").trim_start_matches('/')
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn res_paths() {
        assert_eq!(res_to_path("res://scenes/main.tscn"), "scenes/main.tscn");
        assert_eq!(path_to_res("scenes\\main.tscn"), "res://scenes/main.tscn");
        assert_eq!(path_to_res("res://a.tscn"), "res://a.tscn");
    }
}
