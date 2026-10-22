//! Vim emulation: reading `~/.ideavimrc`. The engine itself (`forge-vim`) runs in the UI
//! process as WebAssembly because every editor key press needs a synchronous answer.

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VimRc {
    /// The file that was read (`None`: no rc file exists).
    pub path: Option<String>,
    /// Its text with `source <file>` lines inlined.
    pub text: String,
}

fn expand_home(path: &str, home: &Path) -> PathBuf {
    match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        Some(rest) => home.join(rest),
        None if path == "~" => home.to_path_buf(),
        None => PathBuf::from(path),
    }
}

/// Where IdeaVim looks for its rc file, in order.
fn candidates(home: &Path, xdg: Option<PathBuf>) -> Vec<PathBuf> {
    let config = xdg.unwrap_or_else(|| home.join(".config"));
    vec![
        home.join(".ideavimrc"),
        home.join("_ideavimrc"),
        config.join("ideavim").join("ideavimrc"),
    ]
}

/// Read the rc file at `custom` (when set) or IdeaVim's default locations.
pub fn read_rc(custom: Option<&str>, home: &Path, xdg: Option<PathBuf>) -> VimRc {
    let paths = match custom.map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => vec![expand_home(p, home)],
        None => candidates(home, xdg),
    };
    for path in paths {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let read = |p: &str| std::fs::read_to_string(expand_home(p, home)).ok();
            return VimRc {
                path: Some(path.display().to_string()),
                text: forge_vim::rc::expand_sources(&text, &read),
            };
        }
    }
    VimRc::default()
}

/// The user's `~/.ideavimrc` (or the file of the `vim.rcFile` setting).
#[tauri::command]
pub fn vim_read_rc(path: Option<String>) -> VimRc {
    let Some(home) = dirs::home_dir() else {
        return VimRc::default();
    };
    let xdg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    read_rc(path.as_deref(), &home, xdg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_first_rc_file_and_inlines_sources() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        assert_eq!(read_rc(None, home, None).path, None);
        std::fs::create_dir_all(home.join(".config/ideavim")).unwrap();
        std::fs::write(home.join(".config/ideavim/ideavimrc"), "set nu").unwrap();
        assert_eq!(read_rc(None, home, None).text, "set nu");
        std::fs::write(home.join(".vimrc"), "set hls").unwrap();
        std::fs::write(home.join(".ideavimrc"), "source ~/.vimrc\nset ic").unwrap();
        let rc = read_rc(None, home, None);
        assert!(rc.path.unwrap().ends_with(".ideavimrc"));
        assert_eq!(rc.text, "set hls\nset ic");
        assert_eq!(read_rc(Some("~/.vimrc"), home, None).text, "set hls");
    }
}
