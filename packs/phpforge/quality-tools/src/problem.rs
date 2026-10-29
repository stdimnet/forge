//! Problems the tools report, with the files mapped back into the project.

use forge_runtargets::mapping::{join, normalize, strip_dir, Mapper};
use serde::{Deserialize, Serialize};

use crate::model::{QualityTool, SeverityChoice};

/// The severities of the editor and the Problems tool window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Error,
    Warning,
    Weak,
    Info,
}

impl Severity {
    /// The severity shown for a problem the tool reported as `self`.
    pub fn with_choice(self, choice: SeverityChoice) -> Severity {
        match choice {
            SeverityChoice::Tool => self,
            SeverityChoice::Error => Severity::Error,
            SeverityChoice::Warning => Severity::Warning,
            SeverityChoice::Weak => Severity::Weak,
        }
    }
}

/// One problem of a tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub tool: QualityTool,
    /// Relative to the project root (absolute when the file is outside of it).
    pub file: String,
    /// 0-based.
    pub line: u32,
    /// 0-based, in characters.
    pub column: u32,
    /// Where the problem ends (0-based, exclusive column); `None`: to the end of `line`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u32>,
    pub severity: Severity,
    pub message: String,
    /// The tool's rule or error identifier (`method.notFound`, `PSR12.Files.EndFileNewline`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    /// The tool (or its fixer, phpcbf) can fix it.
    pub fixable: bool,
    /// Documentation of the rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

/// Where the tool ran, to turn the paths it reports into project paths.
#[derive(Debug, Clone, Default)]
pub struct PathContext {
    /// The local project root (absolute).
    pub root: String,
    /// The tool's working directory as the tool sees it (the remote project root when it
    /// ran through an interpreter); relative paths in reports start there.
    pub cwd: String,
    /// The interpreter's path mappings (empty when the tool ran where the project is).
    pub mapper: Mapper,
    /// Reported names that stand for a project file: `-` and `STDIN` for text read from
    /// stdin, a temporary copy's path. Values are absolute local paths.
    pub aliases: Vec<(String, String)>,
}

impl PathContext {
    /// A context for a tool that ran in the project root on this side.
    pub fn local(root: &str) -> Self {
        Self {
            root: normalize(root),
            cwd: normalize(root),
            ..Self::default()
        }
    }

    /// A reported file as a project path.
    pub fn localize(&self, reported: &str) -> String {
        let reported = reported.trim();
        if let Some((_, local)) = self
            .aliases
            .iter()
            .find(|(alias, _)| alias == reported || normalize(alias) == normalize(reported))
        {
            return self.relative(local);
        }
        let abs = join(&self.cwd, reported);
        let local = self.mapper.to_local(&abs).unwrap_or(abs);
        self.relative(&local)
    }

    fn relative(&self, local: &str) -> String {
        let local = normalize(local);
        match strip_dir(&local, &normalize(&self.root)) {
            Some(rel) if !rel.is_empty() => rel.to_string(),
            _ => local,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_runtargets::PathMapping;

    #[test]
    fn maps_container_paths_back_into_the_project() {
        let ctx = PathContext {
            root: "/home/me/shop".into(),
            cwd: "/opt/project".into(),
            mapper: Mapper::new(
                "/home/me/shop",
                &[PathMapping {
                    local: "".into(),
                    remote: "/opt/project".into(),
                }],
            ),
            aliases: vec![("-".into(), "/home/me/shop/app/A.php".into())],
        };
        assert_eq!(ctx.localize("/opt/project/app/User.php"), "app/User.php");
        assert_eq!(ctx.localize("app/User.php"), "app/User.php");
        assert_eq!(ctx.localize("-"), "app/A.php");
        assert_eq!(ctx.localize("/usr/share/x.php"), "/usr/share/x.php");
        let local = PathContext::local("/home/me/shop/");
        assert_eq!(
            local.localize("/home/me/shop/routes/web.php"),
            "routes/web.php"
        );
        assert_eq!(local.localize("./routes/web.php"), "routes/web.php");
    }
}
