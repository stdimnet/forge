//! The application's MVC files by Rails' conventions: controllers and mailers with their
//! public actions, views and partials in `app/views`, models with their table,
//! associations and scopes, and the `render` / `layout` references of controllers and views.

use serde::{Deserialize, Serialize};

use crate::inflect::{camelize, pluralize, singularize, tableize, underscore};
use crate::schema::Column;
use crate::source::{call_args, code, tokens, LineIndex, Tok, Val};
use crate::Location;

/// A method with its place: an action, a scope, a Stimulus target.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Named {
    pub name: String,
    pub location: Location,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Controller {
    /// `admin/articles`; mailers: `user_mailer`.
    pub name: String,
    /// `Admin::ArticlesController`.
    pub class: String,
    /// `controller` or `mailer`.
    pub kind: String,
    pub path: String,
    pub location: Location,
    /// Public methods.
    pub actions: Vec<Named>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
}

impl Controller {
    pub fn action(&self, name: &str) -> Option<&Named> {
        self.actions.iter().find(|a| a.name == name)
    }
}

/// `app/controllers/admin/articles_controller.rb` → `admin/articles`;
/// `app/mailers/user_mailer.rb` → `user_mailer`.
pub fn controller_name(path: &str) -> Option<(String, &'static str)> {
    if let Some(rest) = path.strip_prefix("app/controllers/") {
        let name = rest.strip_suffix("_controller.rb")?;
        if name.starts_with("concerns/") {
            return None;
        }
        return Some((name.to_string(), "controller"));
    }
    let rest = path.strip_prefix("app/mailers/")?;
    let name = rest.strip_suffix(".rb")?;
    (name != "application_mailer" && name.ends_with("_mailer")).then(|| (name.to_string(), "mailer"))
}

/// Public `def`s of a class (before `private` / `protected`).
pub fn public_methods(path: &str, text: &str) -> Vec<Named> {
    let idx = LineIndex::new(text);
    let mut out = Vec::new();
    let mut offset = 0;
    let mut public = true;
    for line in text.split('\n') {
        let start = offset;
        offset += line.len() + 1;
        let c = code(line);
        let t = c.trim();
        if matches!(t, "private" | "protected") {
            public = false;
            continue;
        }
        if t == "public" {
            public = true;
            continue;
        }
        let Some(rest) = t.strip_prefix("def ") else { continue };
        if !public || rest.starts_with("self.") {
            continue;
        }
        let name: String = rest
            .chars()
            .take_while(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '?' || *ch == '!')
            .collect();
        if name.is_empty() || name == "initialize" {
            continue;
        }
        let at = start + (c.len() - c.trim_start().len()) + 4;
        let (l, ch) = idx.position(at);
        out.push(Named {
            name,
            location: Location {
                path: path.to_string(),
                line: l,
                character: ch,
            },
        });
    }
    out
}

/// The first `class` of a file: (its full name with enclosing modules, superclass, byte
/// offset of the class name).
pub fn first_class(text: &str) -> Option<(String, String, usize)> {
    let mut modules: Vec<String> = Vec::new();
    let mut offset = 0;
    for line in text.split('\n') {
        let start = offset;
        offset += line.len() + 1;
        let c = code(line);
        let t = c.trim_start();
        if let Some(m) = t.strip_prefix("module ") {
            modules.push(m.trim().to_string());
            continue;
        }
        if let Some(rest) = t.strip_prefix("class ") {
            if rest.starts_with("<<") {
                continue;
            }
            let (name, sup) = match rest.split_once('<') {
                Some((n, s)) => (n.trim(), s.trim()),
                None => (rest.trim(), ""),
            };
            let sup = sup.split(|ch: char| ch.is_whitespace() || ch == ';').next().unwrap_or("");
            let mut full = modules.clone();
            full.push(name.to_string());
            let at = start + (c.len() - t.len()) + 6;
            return Some((full.join("::"), sup.to_string(), at));
        }
    }
    None
}

pub fn controller(path: &str, text: &str) -> Option<Controller> {
    let (name, kind) = controller_name(path)?;
    let idx = LineIndex::new(text);
    let (class, location) = match first_class(text) {
        Some((c, _, at)) => {
            let (line, character) = idx.position(at);
            (
                c,
                Location {
                    path: path.to_string(),
                    line,
                    character,
                },
            )
        }
        None => (
            format!("{}Controller", camelize(&name)),
            Location {
                path: path.to_string(),
                ..Location::default()
            },
        ),
    };
    let layout = text.lines().find_map(|l| {
        let t = code(l).trim();
        let rest = t.strip_prefix("layout ")?;
        let toks = tokens(rest);
        match &toks.first()?.tok {
            Tok::Str { value, interp: false } | Tok::Sym(value) => Some(value.clone()),
            _ => None,
        }
    });
    Some(Controller {
        name,
        class,
        kind: kind.into(),
        path: path.to_string(),
        location,
        actions: public_methods(path, text),
        layout,
    })
}

// ---- Views -------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub path: String,
    /// `admin/articles`.
    pub dir: String,
    /// `index`, `form` (partials without the underscore).
    pub name: String,
    pub partial: bool,
}

/// View template extensions (handlers).
pub const VIEW_HANDLERS: &[&str] = &["erb", "haml", "slim", "jbuilder", "builder", "rb"];

pub fn view(path: &str) -> Option<View> {
    let rest = path.strip_prefix("app/views/")?;
    let (dir, file) = rest.rsplit_once('/').unwrap_or(("", rest));
    let handler = file.rsplit('.').next()?;
    if !VIEW_HANDLERS.contains(&handler) || !file.contains('.') {
        return None;
    }
    let stem = file.split('.').next()?;
    let (partial, name) = match stem.strip_prefix('_') {
        Some(n) => (true, n),
        None => (false, stem),
    };
    Some(View {
        path: path.to_string(),
        dir: dir.to_string(),
        name: name.to_string(),
        partial,
    })
}

/// What a `render` (or `layout`) names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RenderKind {
    Partial,
    Template,
    Layout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderRef {
    /// Byte range of the name in the file (string content or the `@ivar`).
    pub start: usize,
    pub end: usize,
    pub name: String,
    pub kind: RenderKind,
    /// The name is literal text (not derived from `@articles`): unresolved ones are reported.
    pub literal: bool,
}

/// `render` / `layout` references in a controller or view.
pub fn render_refs(path: &str, text: &str) -> Vec<RenderRef> {
    let in_view = path.starts_with("app/views/");
    let in_controller = controller_name(path).is_some();
    if !in_view && !in_controller {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut offset = 0;
    for line in text.split('\n') {
        let base = offset;
        offset += line.len() + 1;
        if !line.contains("render") && !line.contains("layout") {
            continue;
        }
        let toks = tokens(line);
        for (i, t) in toks.iter().enumerate() {
            let Some(name) = t.ident() else { continue };
            if !(name == "render" || (name == "layout" && in_controller && i == 0)) {
                continue;
            }
            if i > 0 && toks[i - 1].is_punct('.') {
                continue;
            }
            let rest = &toks[i..];
            let args = call_args(rest);
            let mut j = i + 1;
            if toks.get(j).is_some_and(|t| t.is_punct('(')) {
                j += 1;
            }
            let push = |out: &mut Vec<RenderRef>, tok: &crate::source::Token, value: &str, kind: RenderKind| {
                let (s, e) = match &tok.tok {
                    Tok::Sym(_) => (tok.start + 1, tok.end),
                    _ => (tok.start + 1, tok.end.saturating_sub(1)),
                };
                out.push(RenderRef {
                    start: base + s,
                    end: base + e.max(s),
                    name: value.to_string(),
                    kind,
                    literal: true,
                });
            };
            if name == "layout" {
                if let Some(tok) = toks.get(j) {
                    if let Tok::Str { value, interp: false } | Tok::Sym(value) = &tok.tok {
                        push(&mut out, tok, value, RenderKind::Layout);
                    }
                }
                continue;
            }
            // Keyword forms: `render partial: "x"`, `template:`, `action:`, `layout:`.
            let mut k = j;
            let mut found = false;
            while k + 1 < toks.len() {
                if let Tok::Key(key) = &toks[k].tok {
                    let kind = match key.as_str() {
                        "partial" => Some(RenderKind::Partial),
                        "template" | "action" => Some(RenderKind::Template),
                        "layout" if in_view => Some(RenderKind::Partial),
                        "layout" => Some(RenderKind::Layout),
                        _ => None,
                    };
                    if let (Some(kind), Tok::Str { value, interp: false } | Tok::Sym(value)) = (kind, &toks[k + 1].tok) {
                        push(&mut out, &toks[k + 1], value, kind);
                        found = true;
                    }
                }
                if toks[k].ident().is_some_and(|n| n == "render") {
                    break;
                }
                k += 1;
            }
            if found {
                continue;
            }
            let Some(first) = toks.get(j) else { continue };
            match &first.tok {
                Tok::Str { value, interp: false } => {
                    let kind = if in_view { RenderKind::Partial } else { RenderKind::Template };
                    push(&mut out, first, value, kind);
                }
                Tok::Sym(value) if in_controller => push(&mut out, first, value, RenderKind::Template),
                Tok::Ivar(var) if in_view || args.positional.len() == 1 => {
                    // `render @articles` → articles/_article.
                    let singular = singularize(var);
                    out.push(RenderRef {
                        start: base + first.start,
                        end: base + first.end,
                        name: format!("{}/{singular}", pluralize(&singular)),
                        kind: RenderKind::Partial,
                        literal: false,
                    });
                }
                _ => {}
            }
        }
    }
    out
}

/// The view folder a file renders from: a view's folder, a controller's name.
pub fn view_dir(path: &str) -> Option<String> {
    if let Some(v) = view(path) {
        return Some(v.dir);
    }
    controller_name(path).map(|(n, _)| n)
}

/// Files a render reference can mean, best first.
pub fn resolve_render(path: &str, r: &RenderRef, views: &[View]) -> Vec<String> {
    let dir = view_dir(path).unwrap_or_default();
    let (rdir, base) = match r.name.rsplit_once('/') {
        Some((d, b)) => (d.to_string(), b.to_string()),
        None => (
            match r.kind {
                RenderKind::Layout => "layouts".into(),
                _ => dir.clone(),
            },
            r.name.clone(),
        ),
    };
    let find = |d: &str, partial: bool| -> Vec<String> {
        views
            .iter()
            .filter(|v| v.dir == d && v.name == base && v.partial == partial)
            .map(|v| v.path.clone())
            .collect()
    };
    let partial = r.kind == RenderKind::Partial;
    let mut found = find(&rdir, partial);
    if found.is_empty() && !r.name.contains('/') && partial {
        found = find("application", true);
    }
    found
}

// ---- Models ------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Association {
    /// `belongs_to`, `has_many`, `has_one`, `has_and_belongs_to_many`.
    pub kind: String,
    pub name: String,
    /// The associated model: `class_name:` or the name's class.
    pub class_name: String,
    pub location: Location,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    /// `Article`, `Admin::User`.
    pub name: String,
    pub superclass: String,
    pub path: String,
    pub location: Location,
    pub table: String,
    pub associations: Vec<Association>,
    pub scopes: Vec<Named>,
    /// The table's columns (from schema.rb and migrations).
    pub columns: Vec<Column>,
}

impl Model {
    /// `article` for `Article`, `line_item` for `Shop::LineItem`.
    pub fn variable(&self) -> String {
        underscore(self.name.rsplit("::").next().unwrap_or(&self.name))
    }
}

const BASES: &[&str] = &["ApplicationRecord", "ActiveRecord::Base", "::ApplicationRecord"];

/// Read a model file (`app/models/**/*.rb`); `None` for other classes and abstract ones.
pub fn model(path: &str, text: &str) -> Option<Model> {
    if !path.starts_with("app/models/") || !path.ends_with(".rb") || path.contains("/concerns/") {
        return None;
    }
    let (name, superclass, at) = first_class(text)?;
    if superclass.is_empty() {
        return None;
    }
    if text.contains("self.abstract_class = true") || text.contains("primary_abstract_class") {
        return None;
    }
    let idx = LineIndex::new(text);
    let loc = |byte: usize| {
        let (line, character) = idx.position(byte);
        Location {
            path: path.to_string(),
            line,
            character,
        }
    };
    let mut m = Model {
        table: tableize(&name),
        location: loc(at),
        name,
        superclass,
        path: path.to_string(),
        ..Model::default()
    };
    let mut offset = 0;
    for line in text.split('\n') {
        let start = offset;
        offset += line.len() + 1;
        let c = code(line);
        let t = c.trim_start();
        let indent = c.len() - t.len();
        let toks = tokens(t);
        if t.starts_with("self.table_name") {
            if let Some(Tok::Str { value, .. }) = toks.iter().map(|x| &x.tok).find(|x| matches!(x, Tok::Str { .. })) {
                m.table = value.clone();
            }
            continue;
        }
        let Some(first) = toks.first().and_then(|x| x.ident()) else {
            continue;
        };
        let args = call_args(&toks);
        let first_name = |a: &crate::source::Args| match a.positional.first() {
            Some(Val::Sym(s) | Val::Str(s)) => Some(s.clone()),
            _ => None,
        };
        match first {
            "belongs_to" | "has_many" | "has_one" | "has_and_belongs_to_many" => {
                let Some(n) = first_name(&args) else { continue };
                let class_name = args.text("class_name").map(str::to_string).unwrap_or_else(|| {
                    if first == "has_many" || first == "has_and_belongs_to_many" {
                        camelize(&singularize(&n))
                    } else {
                        camelize(&n)
                    }
                });
                m.associations.push(Association {
                    kind: first.to_string(),
                    name: n,
                    class_name,
                    location: loc(start + indent),
                });
            }
            "scope" => {
                if let Some(n) = first_name(&args) {
                    m.scopes.push(Named {
                        name: n,
                        location: loc(start + indent),
                    });
                }
            }
            _ => {}
        }
    }
    Some(m)
}

/// Resolve single table inheritance (a model whose superclass is another model shares its
/// table) and namespaces with a `table_name_prefix`, then attach the columns.
pub fn link_models(models: &mut [Model], tables: &[crate::schema::Table], prefix_of: &dyn Fn(&str) -> Option<String>) {
    let explicit: Vec<bool> = models.iter().map(|m| m.table != tableize(&m.name)).collect();
    for (i, e) in explicit.iter().enumerate() {
        if *e {
            continue;
        }
        if let Some((ns, _)) = models[i].name.rsplit_once("::") {
            if let Some(prefix) = prefix_of(ns) {
                models[i].table = format!("{prefix}{}", models[i].table);
            }
        }
    }
    for _ in 0..4 {
        for i in 0..models.len() {
            if BASES.contains(&models[i].superclass.as_str()) || explicit[i] {
                continue;
            }
            let sup = models[i].superclass.trim_start_matches("::").to_string();
            if let Some(base) = models.iter().find(|m| m.name == sup || m.name.ends_with(&format!("::{sup}"))) {
                let table = base.table.clone();
                models[i].table = table;
            }
        }
    }
    for m in models.iter_mut() {
        if let Some(t) = tables.iter().find(|t| t.name == m.table) {
            m.columns = t.columns.clone();
        }
    }
}

/// Whether a class (by superclass chain within `models`) is an ActiveRecord model.
pub fn is_record(superclass: &str, models: &[Model]) -> bool {
    BASES.contains(&superclass) || models.iter().any(|m| m.name == superclass)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controllers_and_actions() {
        let src = "module Admin\n  class ArticlesController < ApplicationController\n    layout \"admin\"\n    def index\n    end\n\n    def show; end\n\n    private\n\n    def set_article\n    end\nend\nend\n";
        let c = controller("app/controllers/admin/articles_controller.rb", src).unwrap();
        assert_eq!(c.name, "admin/articles");
        assert_eq!(c.class, "Admin::ArticlesController");
        assert_eq!(c.layout.as_deref(), Some("admin"));
        let names: Vec<&str> = c.actions.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["index", "show"]);
        assert_eq!((c.actions[0].location.line, c.actions[0].location.character), (3, 8));
        assert!(controller("app/controllers/concerns/auth_controller.rb", "").is_none());
        assert_eq!(controller_name("app/mailers/user_mailer.rb").unwrap().0, "user_mailer");
    }

    #[test]
    fn views_and_render_references() {
        let v = view("app/views/articles/_form.html.erb").unwrap();
        assert_eq!((v.dir.as_str(), v.name.as_str(), v.partial), ("articles", "form", true));
        assert!(view("app/views/articles/index.html.erb").is_some_and(|v| !v.partial));
        assert!(view("app/views/articles/README.md").is_none());
        let views: Vec<View> = [
            "app/views/articles/_form.html.erb",
            "app/views/articles/edit.html.erb",
            "app/views/articles/_article.html.erb",
            "app/views/application/_flash.html.erb",
            "app/views/layouts/admin.html.erb",
        ]
        .iter()
        .filter_map(|p| view(p))
        .collect();
        let text = "<%= render \"form\", article: @article %>\n<%= render @articles %>\n<%= render partial: \"flash\" %>\n<%= render \"missing\" %>\n";
        let refs = render_refs("app/views/articles/new.html.erb", text);
        let got: Vec<(String, Vec<String>)> = refs
            .iter()
            .map(|r| (r.name.clone(), resolve_render("app/views/articles/new.html.erb", r, &views)))
            .collect();
        assert_eq!(got[0].1, ["app/views/articles/_form.html.erb"]);
        assert_eq!(&text[refs[0].start..refs[0].end], "form");
        assert_eq!(got[1], ("articles/article".into(), vec!["app/views/articles/_article.html.erb".to_string()]));
        assert_eq!(got[2].1, ["app/views/application/_flash.html.erb"]);
        assert!(got[3].1.is_empty());
        let ctl = "class ArticlesController < ApplicationController\n  layout \"admin\"\n  def update\n    render :edit, status: :unprocessable_entity\n  end\nend\n";
        let refs = render_refs("app/controllers/articles_controller.rb", ctl);
        let got: Vec<Vec<String>> = refs
            .iter()
            .map(|r| resolve_render("app/controllers/articles_controller.rb", r, &views))
            .collect();
        assert_eq!(got, [vec!["app/views/layouts/admin.html.erb".to_string()], vec!["app/views/articles/edit.html.erb".to_string()]]);
    }

    #[test]
    fn models_with_associations_and_sti() {
        let article = "class Article < ApplicationRecord\n  belongs_to :author, class_name: \"User\"\n  has_many :comments, dependent: :destroy\n  scope :published, -> { where(status: :published) }\nend\n";
        let m = model("app/models/article.rb", article).unwrap();
        assert_eq!(m.table, "articles");
        let a: Vec<(&str, &str)> = m.associations.iter().map(|a| (a.name.as_str(), a.class_name.as_str())).collect();
        assert_eq!(a, [("author", "User"), ("comments", "Comment")]);
        assert_eq!(m.scopes[0].name, "published");
        assert!(model("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n").is_none());
        let mut models = vec![
            m,
            model("app/models/featured_article.rb", "class FeaturedArticle < Article\nend\n").unwrap(),
            model("app/models/admin/user.rb", "module Admin\n  class User < ApplicationRecord\n  end\nend\n").unwrap(),
            model("app/models/legacy.rb", "class Legacy < ApplicationRecord\n  self.table_name = \"tbl_legacy\"\nend\n").unwrap(),
        ];
        link_models(&mut models, &[], &|ns| (ns == "Admin").then(|| "admin_".to_string()));
        let tables: Vec<&str> = models.iter().map(|m| m.table.as_str()).collect();
        assert_eq!(tables, ["articles", "articles", "admin_users", "tbl_legacy"]);
        assert_eq!(models[2].variable(), "user");
    }
}
