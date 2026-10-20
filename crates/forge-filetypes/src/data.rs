//! Template Data Languages (as in PHPStorm): the language of the content *between* template
//! tags. Chosen per file from, in order:
//!   1. the `templates.dataLanguages` setting: entries like `legacy/latex/*.tpl=latex`;
//!   2. a compound extension: `invoice.tex.twig`, `query.sql.twig`, `styles.css.tpl`;
//!   3. HTML.

use serde::Serialize;

use crate::glob::matches_glob_list;

/// A data language; the UI maps `id` to its syntax highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DataLanguage {
    pub id: &'static str,
    pub label: &'static str,
}

const fn lang(id: &'static str, label: &'static str) -> DataLanguage {
    DataLanguage { id, label }
}

/// All data languages; the first (HTML) is the default.
pub const DATA_LANGUAGES: &[DataLanguage] = &[
    lang("html", "HTML"),
    lang("xml", "XML"),
    lang("latex", "LaTeX"),
    lang("css", "CSS"),
    lang("scss", "SCSS"),
    lang("less", "Less"),
    lang("javascript", "JavaScript"),
    lang("typescript", "TypeScript"),
    lang("json", "JSON"),
    lang("sql", "SQL"),
    lang("yaml", "YAML"),
    lang("ini", "INI / Properties"),
    lang("shell", "Shell Script"),
    lang("go", "Go"),
    lang("text", "Plain text"),
];

/// Inner extensions of compound file names (`*.tex.twig`) → data language id.
const COMPOUND: &[(&str, &str)] = &[
    ("html", "html"),
    ("htm", "html"),
    ("xhtml", "html"),
    ("xml", "xml"),
    ("svg", "xml"),
    ("rss", "xml"),
    ("atom", "xml"),
    ("tex", "latex"),
    ("latex", "latex"),
    ("sty", "latex"),
    ("cls", "latex"),
    ("css", "css"),
    ("scss", "scss"),
    ("less", "less"),
    ("js", "javascript"),
    ("mjs", "javascript"),
    ("ts", "typescript"),
    ("json", "json"),
    ("sql", "sql"),
    ("yml", "yaml"),
    ("yaml", "yaml"),
    ("ini", "ini"),
    ("env", "ini"),
    ("conf", "ini"),
    ("sh", "shell"),
    ("go", "go"),
    ("txt", "text"),
    ("md", "text"),
    ("csv", "text"),
    ("eml", "text"),
];

/// Data language by id (case-insensitive, surrounding spaces ignored).
pub fn data_language(id: &str) -> Option<DataLanguage> {
    let id = id.trim().to_lowercase();
    DATA_LANGUAGES.iter().copied().find(|l| l.id == id)
}

/// Data language from the file name alone (`report.tex.twig` → LaTeX).
pub fn data_language_from_name(path: &str) -> Option<DataLanguage> {
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    let parts: Vec<&str> = name.split('.').collect();
    // name.<inner>.<template>, e.g. ["invoice", "tex", "twig"]
    if parts.len() < 3 {
        return None;
    }
    let inner = parts[parts.len() - 2];
    COMPOUND
        .iter()
        .find(|(ext, _)| *ext == inner)
        .and_then(|(_, id)| data_language(id))
}

/// Data language from `pattern=language` entries (first match wins).
pub fn data_language_from_mappings(path: &str, mappings: &[String]) -> Option<DataLanguage> {
    mappings.iter().find_map(|entry| {
        let eq = entry.rfind('=').filter(|&i| i > 0)?;
        let lang = data_language(&entry[eq + 1..])?;
        matches_glob_list(path, entry[..eq].trim()).then_some(lang)
    })
}

/// The data language of a template file: mappings, then compound name, then HTML.
pub fn data_language_for(path: &str, mappings: &[String]) -> DataLanguage {
    data_language_from_mappings(path, mappings)
        .or_else(|| data_language_from_name(path))
        .unwrap_or(DATA_LANGUAGES[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(l: Option<DataLanguage>) -> Option<&'static str> {
        l.map(|l| l.id)
    }

    #[test]
    fn detects_compound_extensions() {
        assert_eq!(
            id(data_language_from_name("templates/invoice.tex.twig")),
            Some("latex")
        );
        assert_eq!(id(data_language_from_name("reports/top.sql.twig")), Some("sql"));
        assert_eq!(id(data_language_from_name("theme/style.css.tpl")), Some("css"));
        assert_eq!(id(data_language_from_name("base.html.twig")), Some("html"));
        assert_eq!(id(data_language_from_name("cart.tpl")), None);
        assert_eq!(id(data_language_from_name("cmd/main.go.tmpl")), Some("go"));
        assert_eq!(
            id(data_language_from_name("deploy/config.yaml.tmpl")),
            Some("yaml")
        );
        assert_eq!(id(data_language_from_name("views/index.html.tmpl")), Some("html"));
        assert_eq!(id(data_language_from_name("deploy.tmpl")), None);
    }

    #[test]
    fn applies_pattern_mappings_before_file_names() {
        let m = vec!["legacy/latex/*=latex".to_string(), "*.mail.twig=text".to_string()];
        assert_eq!(
            id(data_language_from_mappings("legacy/latex/letter.tpl", &m)),
            Some("latex")
        );
        assert_eq!(
            id(data_language_from_mappings("emails/welcome.mail.twig", &m)),
            Some("text")
        );
        assert_eq!(id(data_language_from_mappings("templates/base.html.twig", &m)), None);
        assert_eq!(data_language_for("legacy/latex/a.html.twig", &m).id, "latex");
        assert_eq!(data_language_for("index.html.tmpl", &[]).id, "html");
        assert_eq!(data_language_for("query.sql.tmpl", &[]).id, "sql");
        assert_eq!(data_language_for("plain.twig", &[]).id, "html");
    }

    #[test]
    fn ignores_malformed_mappings() {
        let m = vec!["=latex".into(), "*.tpl=klingon".into(), "*.tpl= LaTeX ".into()];
        assert_eq!(id(data_language_from_mappings("a.tpl", &m)), Some("latex"));
    }
}
