//! Ruby LSP (Shopify's `ruby-lsp`, MIT) for Forge: the user's installed `ruby-lsp`
//! (gem executable of the project's Ruby, PATH), or — with the user's consent — an install
//! with the user's own `gem` into Forge's data folder:
//!
//! ```text
//! gem install ruby-lsp --install-dir <data>/forge/langservers/ruby/<ruby version>
//!     --bindir <…>/bin --no-document --env-shebang
//! ```
//!
//! Nothing is bundled or downloaded by Forge itself; RubyGems fetches the gem from
//! rubygems.org (or the user's configured source) and checks it as usual. The installed
//! server runs with `GEM_PATH` = Forge's folder plus the interpreter's own gem path, so it
//! finds its dependencies and the project's bundle.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rubyforge_lang_ruby::RubyCommand;

/// The language server's id in the catalog (`crates/forge-langservers/catalog.toml`).
pub const SERVER_ID: &str = "ruby-lsp";

/// `<data dir>/forge/langservers/ruby`.
pub fn default_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("langservers").join("ruby"))
}

/// The install folder for an interpreter version (gems with native extensions are built
/// per Ruby ABI, so every version gets its own folder).
pub fn install_dir(base: &Path, ruby_version: &str) -> PathBuf {
    let v = if ruby_version.trim().is_empty() {
        "default".to_string()
    } else {
        ruby_version
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    };
    base.join(v)
}

/// The `gem install` command line that installs ruby-lsp into Forge's folder.
pub fn install_command(gem: &str, base: &Path, ruby_version: &str) -> RubyCommand {
    let dir = install_dir(base, ruby_version);
    RubyCommand {
        program: if gem.trim().is_empty() {
            "gem".into()
        } else {
            gem.to_string()
        },
        args: vec![
            "install".into(),
            "ruby-lsp".into(),
            "--install-dir".into(),
            dir.to_string_lossy().into_owned(),
            "--bindir".into(),
            dir.join("bin").to_string_lossy().into_owned(),
            "--no-document".into(),
            "--env-shebang".into(),
        ],
        cwd: String::new(),
        env: BTreeMap::new(),
    }
}

/// The `ruby-lsp` Forge installed for this Ruby version, if any.
pub fn installed(base: &Path, ruby_version: &str, windows: bool) -> Option<PathBuf> {
    let bin = install_dir(base, ruby_version).join("bin");
    [
        if windows { "ruby-lsp.bat" } else { "ruby-lsp" },
        "ruby-lsp",
    ]
    .iter()
    .map(|n| bin.join(n))
    .find(|p| p.is_file())
}

/// The environment a Forge-installed ruby-lsp needs: `GEM_PATH` with Forge's folder and
/// the interpreter's gem path, `PATH` with the interpreter first.
pub fn launch_env(
    exe: &Path,
    gem_path: &[String],
    command_env: &BTreeMap<String, String>,
    windows: bool,
) -> BTreeMap<String, String> {
    let mut env = command_env.clone();
    // `<dir>/bin/ruby-lsp` → `<dir>`.
    if let Some(dir) = exe.parent().and_then(Path::parent) {
        let sep = if windows { ";" } else { ":" };
        let mut parts = vec![dir.to_string_lossy().into_owned()];
        parts.extend(gem_path.iter().cloned());
        env.insert("GEM_PATH".into(), parts.join(sep));
    }
    env
}

/// Whether `exe` is an install of Forge's (it then needs [`launch_env`]).
pub fn is_managed(base: &Path, exe: &Path) -> bool {
    exe.starts_with(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_into_forges_folder_per_ruby() {
        let base = Path::new("/home/anna/.local/share/forge/langservers/ruby");
        let c = install_command("/home/anna/.rbenv/versions/3.3.6/bin/gem", base, "3.3.6");
        assert_eq!(c.program, "/home/anna/.rbenv/versions/3.3.6/bin/gem");
        assert_eq!(
            c.args.join(" "),
            "install ruby-lsp --install-dir /home/anna/.local/share/forge/langservers/ruby/3.3.6 --bindir /home/anna/.local/share/forge/langservers/ruby/3.3.6/bin --no-document --env-shebang"
        );
        assert_eq!(install_dir(base, "jruby 9.4"), base.join("jruby_9.4"));
        let tmp = tempfile::tempdir().unwrap();
        assert!(installed(tmp.path(), "3.3.6", false).is_none());
        let bin = install_dir(tmp.path(), "3.3.6").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("ruby-lsp"), "").unwrap();
        let exe = installed(tmp.path(), "3.3.6", false).unwrap();
        assert!(is_managed(tmp.path(), &exe));
        let env = launch_env(&exe, &["/gems/a".into()], &BTreeMap::new(), false);
        assert_eq!(
            env["GEM_PATH"],
            format!("{}:/gems/a", install_dir(tmp.path(), "3.3.6").display())
        );
    }
}
