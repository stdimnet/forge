//! Languages that can be injected into string literals.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Language {
    /// Id of the editor's syntax mode (`sql`, `html`, `regexp`, ...).
    pub id: &'static str,
    /// Display name, also written into `// language=` comments.
    pub label: &'static str,
    /// Other names accepted in comments and heredoc labels (lower case).
    #[serde(skip)]
    pub aliases: &'static [&'static str],
}

pub const LANGUAGES: &[Language] = &[
    Language {
        id: "sql",
        label: "SQL",
        aliases: &[
            "mysql",
            "mariadb",
            "postgresql",
            "postgres",
            "pgsql",
            "sqlite",
            "genericsql",
            "sql92",
            "tsql",
        ],
    },
    Language {
        id: "html",
        label: "HTML",
        aliases: &["xhtml", "html5"],
    },
    Language {
        id: "css",
        label: "CSS",
        aliases: &[],
    },
    Language {
        id: "json",
        label: "JSON",
        aliases: &["json5"],
    },
    Language {
        id: "regexp",
        label: "RegExp",
        aliases: &["regex", "re", "regularexpression"],
    },
    Language {
        id: "javascript",
        label: "JavaScript",
        aliases: &["js", "ecmascript", "ecmascript6"],
    },
    Language {
        id: "xml",
        label: "XML",
        aliases: &[],
    },
];

/// The language called `name` in a comment or heredoc label, ignoring case.
pub fn language(name: &str) -> Option<&'static Language> {
    let name = name.trim().to_ascii_lowercase();
    LANGUAGES.iter().find(|l| {
        l.id == name || l.label.eq_ignore_ascii_case(&name) || l.aliases.contains(&name.as_str())
    })
}

/// The language with editor id `id`.
pub fn by_id(id: &str) -> Option<&'static Language> {
    LANGUAGES.iter().find(|l| l.id == id)
}
