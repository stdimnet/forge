use std::path::PathBuf;

/// Errors surfaced by the core. Serializable so they can cross the IPC boundary as strings.
#[derive(Debug, thiserror::Error)]
pub enum ForgeError {
    #[error("path is outside of the workspace: {0}")]
    OutsideWorkspace(PathBuf),
    #[error("not found: {0}")]
    NotFound(PathBuf),
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Other(String),
}

impl ForgeError {
    /// "No project is open", in the IDE language.
    pub fn no_project() -> Self {
        Self::Other(forge_i18n::t("errors.noProject"))
    }

    /// The message shown to the user, in the language of the process-wide localizer
    /// (`errors.*`); `Display` stays English for logs.
    pub fn localized(&self) -> String {
        let l10n = forge_i18n::current();
        match self {
            Self::OutsideWorkspace(p) => l10n.t_args(
                "errors.outsideWorkspace",
                &forge_i18n::args! { "path" => p.display().to_string() },
            ),
            Self::NotFound(p) => l10n.t_args(
                "errors.notFound",
                &forge_i18n::args! { "path" => p.display().to_string() },
            ),
            Self::Io { path, source } => l10n.t_args(
                "errors.io",
                &forge_i18n::args! { "path" => path.display().to_string(), "error" => source.to_string() },
            ),
            Self::Other(s) => s.clone(),
        }
    }

    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

impl serde::Serialize for ForgeError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.localized())
    }
}

pub type Result<T> = std::result::Result<T, ForgeError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_messages_match_display() {
        // The process-wide localizer is English unless the app selects another language.
        let errors = [
            ForgeError::NotFound("src/a.php".into()),
            ForgeError::OutsideWorkspace("/etc/passwd".into()),
            ForgeError::io("x.txt", std::io::Error::other("denied")),
        ];
        for e in errors {
            assert_eq!(e.localized(), e.to_string());
        }
        assert_eq!(ForgeError::no_project().to_string(), "no project is open");
    }
}
