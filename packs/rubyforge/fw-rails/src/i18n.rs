//! Rails I18n: the keys of `config/locales/**/*.yml` per locale, the default locale
//! (`config.i18n.default_locale`), and the `t(…)` / `I18n.t(…)` / `translate(…)` calls of
//! views and controllers, with lazy lookup (`t(".title")` in `articles/index.html.erb` is
//! `articles.index.title`).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::source::{tokens, Tok};
use crate::yaml::{self, Value};
use crate::Location;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Translation {
    pub locale: String,
    pub key: String,
    pub value: String,
    pub location: Location,
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub default_locale: String,
    pub locales: Vec<String>,
    /// Key (without the locale) → its translations.
    pub entries: BTreeMap<String, Vec<Translation>>,
    /// Every key prefix that has children (`articles`, `articles.index`).
    namespaces: BTreeSet<String>,
}

/// Plural forms a key's children can have; the key itself then has a translation.
const PLURAL: &[&str] = &["zero", "one", "two", "few", "many", "other"];

/// Keys of the rails-i18n / Rails defaults that projects use without defining them.
const FRAMEWORK: &[&str] = &[
    "activerecord.",
    "activemodel.",
    "errors.",
    "helpers.",
    "date.",
    "time.",
    "datetime.",
    "number.",
    "support.",
    "i18n.",
];

fn display(v: &Value) -> Option<String> {
    match v {
        Value::Scalar(s) => Some(s.clone()),
        Value::Seq(items) => Some(
            items
                .iter()
                .filter_map(|i| i.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        ),
        Value::Map(m) if !m.is_empty() && m.iter().all(|e| PLURAL.contains(&e.key.as_str())) => m
            .iter()
            .find(|e| e.key == "other")
            .or(m.first())
            .and_then(|e| e.value.as_str().map(str::to_string)),
        _ => None,
    }
}

impl Catalog {
    /// Read the locale files: (path, text) of `config/locales/**/*.yml`.
    pub fn new<'a>(files: impl Iterator<Item = (&'a str, &'a str)>, default_locale: &str) -> Self {
        let mut c = Catalog {
            default_locale: default_locale.to_string(),
            ..Catalog::default()
        };
        let mut locales = BTreeSet::new();
        for (path, text) in files {
            let doc = yaml::parse(text);
            for e in doc.entries() {
                locales.insert(e.key.clone());
                c.walk(path, &e.key, "", &e.value, e.line, e.col);
            }
        }
        c.locales = locales.into_iter().collect();
        // The default locale first.
        if let Some(i) = c.locales.iter().position(|l| *l == c.default_locale) {
            let d = c.locales.remove(i);
            c.locales.insert(0, d);
        }
        c
    }

    fn walk(&mut self, path: &str, locale: &str, prefix: &str, v: &Value, line: u32, col: u32) {
        if !prefix.is_empty() {
            if let Some(text) = display(v) {
                self.entries.entry(prefix.to_string()).or_default().push(Translation {
                    locale: locale.to_string(),
                    key: prefix.to_string(),
                    value: text,
                    location: Location {
                        path: path.to_string(),
                        line,
                        character: col,
                    },
                });
            }
        }
        if let Value::Map(m) = v {
            if !prefix.is_empty() {
                self.namespaces.insert(prefix.to_string());
            }
            for e in m {
                let key = if prefix.is_empty() {
                    e.key.clone()
                } else {
                    format!("{prefix}.{}", e.key)
                };
                self.walk(path, locale, &key, &e.value, e.line, e.col);
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<&[Translation]> {
        self.entries.get(key).map(Vec::as_slice)
    }

    /// The text of a key in the default locale (else any).
    pub fn text(&self, key: &str) -> Option<&str> {
        let list = self.get(key)?;
        list.iter()
            .find(|t| t.locale == self.default_locale)
            .or(list.first())
            .map(|t| t.value.as_str())
    }

    pub fn is_namespace(&self, key: &str) -> bool {
        self.namespaces.contains(key)
    }

    /// Locales that miss a key the catalogue knows elsewhere.
    pub fn missing_locales(&self, key: &str) -> Vec<String> {
        let have: BTreeSet<&str> = self
            .get(key)
            .unwrap_or_default()
            .iter()
            .map(|t| t.locale.as_str())
            .collect();
        self.locales
            .iter()
            .filter(|l| !have.contains(l.as_str()))
            .cloned()
            .collect()
    }

    pub fn is_framework_key(key: &str) -> bool {
        FRAMEWORK.iter().any(|p| key.starts_with(p))
    }

    /// Keys for completion: (key, text in the default locale).
    pub fn keys(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().filter_map(|(k, v)| {
            v.iter()
                .find(|t| t.locale == self.default_locale)
                .or(v.first())
                .map(|t| (k.as_str(), t.value.as_str()))
        })
    }
}

/// `config.i18n.default_locale = :de` (application.rb, an environment or initializer) or
/// `I18n.default_locale = :de`; else `en`.
pub fn default_locale<'a>(files: impl Iterator<Item = (&'a str, &'a str)>) -> String {
    for (path, text) in files {
        if !(path.starts_with("config/") && path.ends_with(".rb")) {
            continue;
        }
        for line in text.lines() {
            let t = crate::source::code(line).trim();
            let Some(rest) = t
                .strip_prefix("config.i18n.default_locale")
                .or_else(|| t.strip_prefix("I18n.default_locale"))
            else {
                continue;
            };
            let Some(v) = rest.trim().strip_prefix('=') else { continue };
            let v = v.trim().trim_start_matches(':').trim_matches(['"', '\'']);
            if !v.is_empty() && v.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                return v.to_string();
            }
        }
    }
    "en".into()
}

/// A translation call: `t(".title")`, `I18n.t("x.y")`, `translate :x`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TRef {
    /// Byte range of the key text (inside the quotes).
    pub start: usize,
    pub end: usize,
    /// As written.
    pub key: String,
    /// With lazy lookup expanded (`articles.index.title`); `None` when it cannot be.
    pub full: Option<String>,
    /// Byte range to fold: the whole call when it has no other arguments, else the string.
    pub fold_start: usize,
    pub fold_end: usize,
}

/// The lazy lookup scope of a view or a controller action: `articles.index`,
/// `admin.users.form` (partials drop the underscore).
pub fn lazy_scope(path: &str, text: &str, at: usize) -> Option<String> {
    if let Some(v) = crate::app::view(path) {
        let dir = v.dir.replace('/', ".");
        return Some(if dir.is_empty() { v.name } else { format!("{dir}.{}", v.name) });
    }
    let (name, _) = crate::app::controller_name(path)?;
    let action = crate::editor::enclosing_def(text, at)?;
    Some(format!("{}.{action}", name.replace('/', ".")))
}

/// The translation calls of a file.
pub fn refs(path: &str, text: &str) -> Vec<TRef> {
    let mut out = Vec::new();
    let mut offset = 0;
    for line in text.split('\n') {
        let base = offset;
        offset += line.len() + 1;
        if !line.contains('t') {
            continue;
        }
        let toks = tokens(line);
        for (i, t) in toks.iter().enumerate() {
            let Some(name) = t.ident() else { continue };
            if !matches!(name, "t" | "translate") {
                continue;
            }
            // `I18n.t`, but not `form.t` or `x.translate`.
            let mut call_start = t.start;
            if i > 0 && toks[i - 1].is_punct('.') {
                match toks.get(i.wrapping_sub(2)).map(|x| &x.tok) {
                    Some(Tok::Const(c)) if c == "I18n" => call_start = toks[i - 2].start,
                    _ => continue,
                }
            }
            let mut j = i + 1;
            let paren = toks.get(j).is_some_and(|x| x.is_punct('(') && x.start == t.end);
            if paren {
                j += 1;
            } else if toks.get(j).is_some_and(|x| x.start == t.end) {
                // `t.foo`, `t_x`: not a call with a key.
                continue;
            }
            let Some(arg) = toks.get(j) else { continue };
            let (key, s, e) = match &arg.tok {
                Tok::Str { value, interp: false } => (value.clone(), arg.start + 1, arg.end.saturating_sub(1)),
                Tok::Sym(value) => (value.clone(), arg.start + 1, arg.end),
                _ => continue,
            };
            let whole = paren && toks.get(j + 1).is_some_and(|x| x.is_punct(')'));
            let (fs, fe) = if whole {
                (call_start, toks[j + 1].end)
            } else {
                (arg.start, arg.end)
            };
            let full = if let Some(rel) = key.strip_prefix('.') {
                lazy_scope(path, text, base + t.start).map(|scope| format!("{scope}.{rel}"))
            } else {
                Some(key.clone())
            };
            out.push(TRef {
                start: base + s,
                end: base + e.max(s),
                key,
                full,
                fold_start: base + fs,
                fold_end: base + fe,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        let en = "en:\n  articles:\n    index:\n      title: Listing articles\n      count:\n        one: \"%{count} article\"\n        other: \"%{count} articles\"\n  hello: Hello\n";
        let de = "de:\n  articles:\n    index:\n      title: Artikel\n";
        Catalog::new(
            [("config/locales/en.yml", en), ("config/locales/de.yml", de)].into_iter(),
            "en",
        )
    }

    #[test]
    fn keys_values_and_missing_locales() {
        let c = catalog();
        assert_eq!(c.locales, ["en", "de"]);
        assert_eq!(c.text("articles.index.title"), Some("Listing articles"));
        assert_eq!(c.text("articles.index.count"), Some("%{count} articles"));
        assert!(c.is_namespace("articles.index"));
        assert_eq!(c.missing_locales("hello"), ["de"]);
        let t = &c.get("articles.index.title").unwrap()[1];
        assert_eq!((t.locale.as_str(), t.location.line, t.location.character), ("de", 3, 6));
        assert!(Catalog::is_framework_key("helpers.submit.create"));
    }

    #[test]
    fn calls_and_lazy_lookup() {
        let view = "<h1><%= t(\".title\") %></h1>\n<p><%= t \"hello\", name: x %> <%= I18n.t(:hello) %> <%= form.t(\"x\") %></p>\n";
        let r = refs("app/views/articles/index.html.erb", view);
        let keys: Vec<(&str, Option<&str>)> = r.iter().map(|x| (x.key.as_str(), x.full.as_deref())).collect();
        assert_eq!(
            keys,
            [(".title", Some("articles.index.title")), ("hello", Some("hello")), ("hello", Some("hello"))]
        );
        assert_eq!(&view[r[0].fold_start..r[0].fold_end], "t(\".title\")");
        assert_eq!(&view[r[1].fold_start..r[1].fold_end], "\"hello\"");
        assert_eq!(&view[r[2].fold_start..r[2].fold_end], "I18n.t(:hello)");
        let ctl = "class Admin::UsersController < ApplicationController\n  def create\n    flash[:notice] = t(\".created\")\n  end\nend\n";
        let r = refs("app/controllers/admin/users_controller.rb", ctl);
        assert_eq!(r[0].full.as_deref(), Some("admin.users.create.created"));
        let partial = refs("app/views/articles/_form.html.erb", "<%= t('.save') %>");
        assert_eq!(partial[0].full.as_deref(), Some("articles.form.save"));
        assert_eq!(default_locale([("config/application.rb", "    config.i18n.default_locale = :de\n")].into_iter()), "de");
    }
}
