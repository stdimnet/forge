//! Editor services for PHP and Blade buffers: completion of route names, views, config and
//! env keys, translations, columns, relations, model properties, scopes, components and
//! custom directives; Go to Declaration for the same; gutter markers for routed
//! controller methods, route definitions and `view()` calls.

use phpforge_lang_php::parser::index_source;
use phpforge_lang_php::{SymbolKind, Visibility};
use serde::{Deserialize, Serialize};

use crate::context::{context_at, Context, PropertyAccess, StringArg};
use crate::eloquent::{snake, Model};
use crate::project::{read, Location};
use crate::routes::singular;
use crate::views::resolve_view_path;
use crate::Laravel;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,
    /// `route`, `view`, `config`, `env`, `translation`, `column`, `relation`, `property`,
    /// `method`, `component`, `directive`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    /// Text to insert when it differs from the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert_text: Option<String>,
}

/// Completion items replacing the byte range `from..to` of the buffer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Completions {
    pub from: usize,
    pub to: usize,
    pub items: Vec<CompletionItem>,
}

/// A gutter icon in the editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GutterMark {
    /// 0-based line in the buffer.
    pub line: u32,
    /// `route` (a routed controller method), `controller` (a route definition) or `view`.
    pub kind: String,
    pub tooltip: String,
    pub targets: Vec<Location>,
}

fn item(label: impl Into<String>, kind: &str, detail: Option<String>) -> CompletionItem {
    CompletionItem {
        label: label.into(),
        kind: kind.into(),
        detail,
        documentation: None,
        insert_text: None,
    }
}

/// What a string argument refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StringKind {
    Route,
    View,
    Config,
    Env,
    Translation,
    Column,
    Relation,
}

fn string_kind(s: &StringArg) -> Option<StringKind> {
    let recv = s.receiver.as_deref().unwrap_or("");
    let recv_class = recv.rsplit('\\').next().unwrap_or(recv);
    let first = s.arg_index == 0;
    use StringKind::*;
    Some(match (s.func.as_str(), first) {
        ("route" | "to_route" | "routeIs" | "signedRoute" | "temporarySignedRoute", true) => Route,
        ("has" | "is" | "currentRouteNamed", true) if recv_class == "Route" => Route,
        ("view", _) if recv_class == "Route" => {
            if s.arg_index == 1 {
                View
            } else {
                return None;
            }
        }
        (
            "view" | "markdown" | "@include" | "@includeIf" | "@extends" | "@each" | "@component"
            | "@includeFirst",
            true,
        ) => View,
        ("@includeWhen" | "@includeUnless", false) if s.arg_index == 1 => View,
        ("make" | "exists" | "first", true) if recv_class == "View" => View,
        ("config", true) => Config,
        ("get" | "set" | "has" | "string" | "integer" | "boolean" | "array" | "float", true)
            if recv_class == "Config" || recv == "config()" =>
        {
            Config
        }
        ("env", true) => Env,
        ("__" | "trans" | "trans_choice" | "@lang" | "@choice", true) => Translation,
        ("get" | "has" | "choice", true) if recv_class == "Lang" => Translation,
        (
            "where" | "orWhere" | "whereNot" | "orWhereNot" | "whereIn" | "whereNotIn"
            | "orWhereIn" | "whereNull" | "whereNotNull" | "orWhereNull" | "whereBetween"
            | "whereNotBetween" | "whereDate" | "whereMonth" | "whereDay" | "whereYear"
            | "whereTime" | "whereColumn" | "whereLike" | "orderBy" | "orderByDesc" | "latest"
            | "oldest" | "groupBy" | "pluck" | "value" | "increment" | "decrement" | "firstWhere"
            | "sum" | "avg" | "min" | "max" | "average" | "sortBy" | "sortByDesc" | "keyBy"
            | "unique",
            true,
        ) if !recv.is_empty() => Column,
        ("select" | "addSelect" | "only" | "makeHidden" | "makeVisible" | "groupBy", _)
            if !recv.is_empty() =>
        {
            Column
        }
        (
            "with" | "load" | "loadMissing" | "loadCount" | "withCount" | "has" | "whereHas"
            | "orWhereHas" | "doesntHave" | "whereDoesntHave" | "whereRelation" | "orWhereRelation"
            | "withWhereHas" | "without" | "withSum" | "withAvg" | "withMax" | "withMin"
            | "withExists",
            true,
        ) if !recv.is_empty() => Relation,
        _ => return None,
    })
}

/// Names a buffer imports: `(local name, fqn)`, plus the buffer's namespace.
struct Imports {
    namespace: String,
    uses: Vec<(String, String)>,
}

impl Imports {
    fn of(path: &str, src: &str) -> Self {
        let idx = index_source(path, src);
        Self {
            namespace: idx
                .namespaces
                .first()
                .map(|n| n.name.clone())
                .unwrap_or_default(),
            uses: idx
                .uses
                .iter()
                .map(|u| (u.local_name().to_string(), u.fqn.clone()))
                .collect(),
        }
    }

    fn resolve(&self, name: &str) -> String {
        let name = name.trim();
        if let Some(n) = name.strip_prefix('\\') {
            return n.to_string();
        }
        let (head, rest) = name
            .split_once('\\')
            .map_or((name, None), |(h, r)| (h, Some(r)));
        if let Some((_, fqn)) = self.uses.iter().find(|(l, _)| l.eq_ignore_ascii_case(head)) {
            return match rest {
                Some(r) => format!("{fqn}\\{r}"),
                None => fqn.clone(),
            };
        }
        if self.namespace.is_empty() {
            name.to_string()
        } else {
            format!("{}\\{name}", self.namespace)
        }
    }
}

/// Validation rules for `validationRule` completions of `ide.json`.
const VALIDATION_RULES: &[&str] = &[
    "accepted", "active_url", "after", "alpha", "alpha_dash", "alpha_num", "array", "before",
    "between", "boolean", "confirmed", "date", "decimal", "different", "digits", "email",
    "enum", "exists", "file", "filled", "image", "in", "integer", "ip", "json", "max",
    "mimes", "min", "not_in", "nullable", "numeric", "present", "regex", "required",
    "required_if", "same", "size", "sometimes", "string", "timezone", "unique", "url", "uuid",
];

const PHP_KEYWORDS: &[&str] = &[
    "as",
    "return",
    "echo",
    "new",
    "instanceof",
    "case",
    "use",
    "global",
    "static",
    "print",
    "yield",
    "throw",
    "else",
    "and",
    "or",
    "xor",
    "clone",
    "include",
    "require",
    "fn",
    "function",
];

impl Laravel {
    fn model_by(&self, imports: &Imports, name: &str) -> Option<&Model> {
        self.model(&imports.resolve(name))
            .or_else(|| self.model(name))
    }

    /// Class of `$var` at `offset`: `@var`, parameter types, assignments, then a model
    /// named like the variable (`$order` → `Order`, `$orders` in a loop → `Order`).
    fn variable_model(
        &self,
        src: &str,
        imports: &Imports,
        path: &str,
        var: &str,
        offset: usize,
    ) -> Option<&Model> {
        let before = &src[..offset.min(src.len())];
        if var == "$this" {
            let idx = index_source(path, src);
            let class = idx.class_at(offset)?;
            return self.model(&class.fqn);
        }
        // /** @var Order $order */ or @var Order|null $order
        let mut search = before;
        while let Some(i) = search.rfind(var) {
            let head = &search[..i];
            let after_ok = search[i + var.len()..]
                .chars()
                .next()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
            search = head;
            if !after_ok {
                continue;
            }
            let t = head.trim_end();
            // `Type $var` (parameter or @var)
            let word_start = t
                .rfind(|c: char| {
                    !(c.is_alphanumeric() || c == '_' || c == '\\' || c == '|' || c == '?')
                })
                .map_or(0, |p| p + 1);
            let ty = &t[word_start..];
            let ty = ty
                .trim_start_matches('?')
                .split('|')
                .find(|p| *p != "null")
                .unwrap_or("");
            if !ty.is_empty()
                && ty
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphabetic() || c == '\\')
                && !PHP_KEYWORDS.contains(&ty.to_ascii_lowercase().as_str())
            {
                if let Some(m) = self.model_by(imports, ty) {
                    return Some(m);
                }
            }
            // `$var = new Order` / `$var = Order::find(...)`
            let rest = &src[head.len() + var.len()..];
            let rest = rest.trim_start();
            if let Some(rhs) = rest.strip_prefix('=').filter(|r| !r.starts_with('=')) {
                let rhs = rhs.trim_start();
                let rhs = rhs.strip_prefix("new ").unwrap_or(rhs).trim_start();
                let name: String = rhs
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '\\')
                    .collect();
                if !name.is_empty()
                    && name
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_uppercase() || c == '\\')
                {
                    if let Some(m) = self.model_by(imports, &name) {
                        return Some(m);
                    }
                }
            }
        }
        // Naming convention.
        let bare = var.trim_start_matches('$');
        let single = singular(&snake(bare));
        self.models.iter().find(|m| snake(&m.name) == single)
    }

    /// The model a query builder / relation chain works on.
    fn receiver_model(
        &self,
        src: &str,
        imports: &Imports,
        path: &str,
        receiver: &str,
        offset: usize,
    ) -> Option<&Model> {
        let receiver = receiver.trim();
        // Split into the base and `->name(` segments.
        let (base, rest) = match receiver.find("->") {
            Some(i) => (&receiver[..i], &receiver[i..]),
            None => (receiver, ""),
        };
        let mut model = if let Some((class, _)) = base.split_once("::") {
            self.model_by(imports, class)?
        } else if base.starts_with(|c: char| c.is_ascii_uppercase() || c == '\\') {
            // `Order::with(` has the receiver `Order`.
            self.model_by(imports, base)?
        } else if base.starts_with('$') {
            let var: String = base
                .chars()
                .take_while(|c| *c == '$' || c.is_alphanumeric() || *c == '_')
                .collect();
            self.variable_model(src, imports, path, &var, offset)?
        } else {
            return None;
        };
        // Follow relations: `$user->orders()->where(` / `$user->orders->where(`.
        for seg in rest.split("->").filter(|s| !s.is_empty()) {
            let name: String = seg
                .trim_start_matches('?')
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if let Some(rel) = model.relations.iter().find(|r| r.name == name) {
                if let Some(next) = rel.related.as_deref().and_then(|r| self.model(r)) {
                    model = next;
                }
            }
        }
        Some(model)
    }

    fn property_model(
        &self,
        src: &str,
        imports: &Imports,
        path: &str,
        p: &PropertyAccess,
        offset: usize,
    ) -> Option<&Model> {
        let mut model = self.variable_model(src, imports, path, &p.variable, offset)?;
        for seg in &p.path {
            let rel = model.relations.iter().find(|r| &r.name == seg)?;
            model = self.model(rel.related.as_deref()?)?;
        }
        Some(model)
    }

    fn model_columns(model: &Model) -> Vec<CompletionItem> {
        model
            .attributes
            .iter()
            .filter(|a| !a.accessor)
            .map(|a| {
                let mut it = item(&a.name, "column", Some(short_type(&a.type_name)));
                it.documentation = Some(format!(
                    "{}.{}{}",
                    model.table,
                    a.name,
                    a.column
                        .as_ref()
                        .map(|c| format!(" ({c})"))
                        .unwrap_or_default()
                ));
                it
            })
            .collect()
    }

    fn model_relations(model: &Model) -> Vec<CompletionItem> {
        model
            .relations
            .iter()
            .map(|r| {
                let related = r
                    .related
                    .as_deref()
                    .map(|c| c.rsplit('\\').next().unwrap_or(c).to_string());
                item(
                    &r.name,
                    "relation",
                    Some(format!(
                        "{}{}",
                        r.kind,
                        related.map(|c| format!(" → {c}")).unwrap_or_default()
                    )),
                )
            })
            .collect()
    }

    /// Completion at a byte offset of a PHP or Blade buffer.
    /// Values `ide.json` completions offer for a string argument.
    fn ide_items(&self, s: &StringArg, imports: &Imports, path: &str, src: &str) -> Vec<CompletionItem> {
        if self.ide.completions.is_empty() {
            return Vec::new();
        }
        let receiver = s.receiver.as_deref().unwrap_or("");
        let (class, parents) = if s.is_static && !receiver.contains(['(', '$']) {
            (Some(imports.resolve(receiver)), Vec::new())
        } else if receiver == "$this" {
            let idx = index_source(path, src);
            match idx.class_at(s.start) {
                Some(c) => (
                    Some(c.fqn.clone()),
                    c.extends.iter().map(|e| imports.resolve(e)).collect(),
                ),
                None => (None, Vec::new()),
            }
        } else {
            (None, Vec::new())
        };
        let site = crate::ide_json::CallSite {
            function: &s.func,
            class,
            parents,
            is_method: s.receiver.is_some(),
            arg_index: s.arg_index,
        };
        let mut items: Vec<CompletionItem> = Vec::new();
        for c in crate::ide_json::applicable(&self.ide, &site) {
            let detail = Some(c.source.clone());
            let values: Vec<(String, Option<String>)> = match c.complete.as_str() {
                "staticStrings" => c.strings.iter().map(|v| (v.clone(), None)).collect(),
                "directoryFiles" => crate::ide_json::directory_values(&self.root, c)
                    .into_iter()
                    .map(|v| (v, None))
                    .collect(),
                "routeName" => self
                    .routes
                    .iter()
                    .filter_map(|r| r.name.clone().map(|n| (n, Some(r.uri.clone()))))
                    .collect(),
                "viewName" => self.views.iter().map(|v| (v.name.clone(), None)).collect(),
                "configKey" => self
                    .config
                    .iter()
                    .map(|k| (k.key.clone(), Some(k.value.clone())))
                    .collect(),
                "translationKey" => self
                    .translations
                    .iter()
                    .map(|t| (t.key.clone(), Some(t.value.clone())))
                    .collect(),
                "envKey" => self
                    .env
                    .iter()
                    .map(|e| (e.key.clone(), Some(e.value.clone())))
                    .collect(),
                "validationRule" => VALIDATION_RULES.iter().map(|r| (r.to_string(), None)).collect(),
                _ => Vec::new(),
            };
            for (label, doc) in values {
                if items.iter().any(|i| i.label == label) {
                    continue;
                }
                let mut it = item(label, "value", detail.clone());
                it.documentation = doc;
                items.push(it);
            }
        }
        items
    }

    pub fn complete(&self, path: &str, src: &str, offset: usize) -> Option<Completions> {
        let ctx = context_at(src, offset)?;
        let imports = Imports::of(path, src);
        match ctx {
            Context::String(s) => {
                let Some(kind) = string_kind(&s) else {
                    let items = self.ide_items(&s, &imports, path, src);
                    return (!items.is_empty()).then_some(Completions {
                        from: s.start,
                        to: s.end,
                        items,
                    });
                };
                let items: Vec<CompletionItem> = match kind {
                    StringKind::Route => self
                        .routes
                        .iter()
                        .filter_map(|r| {
                            let name = r.name.as_ref()?;
                            let mut it = item(
                                name,
                                "route",
                                Some(format!(
                                    "{} /{}",
                                    r.method_label(),
                                    r.uri.trim_start_matches('/')
                                )),
                            );
                            it.documentation = Some(r.action.clone());
                            Some(it)
                        })
                        .collect(),
                    StringKind::View => self
                        .views
                        .iter()
                        .map(|v| item(&v.name, "view", Some(v.path.clone())))
                        .collect(),
                    StringKind::Config => self
                        .config
                        .iter()
                        .map(|c| item(&c.key, "config", Some(c.value.clone())))
                        .collect(),
                    StringKind::Env => self
                        .env
                        .iter()
                        .map(|e| item(&e.key, "env", Some(e.value.clone())))
                        .collect(),
                    StringKind::Translation => self
                        .translations
                        .iter()
                        .map(|t| {
                            let mut it = item(&t.key, "translation", Some(t.value.clone()));
                            it.documentation = Some(t.locales.join(", "));
                            it
                        })
                        .collect(),
                    StringKind::Column => {
                        let m = self.receiver_model(
                            src,
                            &imports,
                            path,
                            s.receiver.as_deref()?,
                            offset,
                        )?;
                        Self::model_columns(m)
                    }
                    StringKind::Relation => {
                        let m = self.receiver_model(
                            src,
                            &imports,
                            path,
                            s.receiver.as_deref()?,
                            offset,
                        )?;
                        Self::model_relations(m)
                    }
                };
                Some(Completions {
                    from: s.start,
                    to: s.end,
                    items,
                })
            }
            Context::ActionMethod(a) => {
                let class = imports.resolve(&a.class);
                let rel = self.psr4.class_path(&self.root, &class)?;
                let csrc = read(&self.root, &rel)?;
                let idx = index_source(&rel, &csrc);
                let c = idx.classes.first()?;
                let items = c
                    .methods()
                    .filter(|m| m.visibility == Visibility::Public && !m.name.starts_with("__"))
                    .map(|m| item(&m.name, "method", Some(m.signature())))
                    .collect();
                Some(Completions {
                    from: a.start,
                    to: a.end,
                    items,
                })
            }
            Context::Property(p) => {
                let m = self.property_model(src, &imports, path, &p, offset)?;
                let mut items: Vec<CompletionItem> = m
                    .attributes
                    .iter()
                    .map(|a| {
                        let mut it = item(&a.name, "property", Some(short_type(&a.type_name)));
                        it.documentation = Some(if a.accessor {
                            format!("Accessor of {}", m.name)
                        } else {
                            format!("{}.{}", m.table, a.name)
                        });
                        it
                    })
                    .collect();
                for r in &m.relations {
                    let related = r
                        .related
                        .as_deref()
                        .map(|c| c.rsplit('\\').next().unwrap_or(c).to_string())
                        .unwrap_or_else(|| "Model".into());
                    let ty = if r.is_many() {
                        format!("Collection<{related}>")
                    } else {
                        related
                    };
                    let mut it = item(&r.name, "property", Some(ty));
                    it.documentation = Some(format!("{} relation", r.kind));
                    items.push(it);
                }
                Some(Completions {
                    from: p.start,
                    to: p.start + p.prefix.len(),
                    items,
                })
            }
            Context::Static {
                class,
                start,
                prefix,
            } => {
                let m = self.model_by(&imports, &class)?;
                let items = m
                    .scopes
                    .iter()
                    .map(|s| {
                        let mut it = item(
                            &s.name,
                            "method",
                            Some(format!("scope({})", s.params.join(", "))),
                        );
                        it.insert_text = Some(format!("{}()", s.name));
                        it
                    })
                    .collect();
                Some(Completions {
                    from: start,
                    to: start + prefix.len(),
                    items,
                })
            }
            Context::ComponentTag { start, end, .. } => {
                let items = self
                    .components
                    .iter()
                    .map(|c| {
                        let detail = c.class.clone().or_else(|| c.view.clone());
                        let mut it = item(&c.tag, "component", detail);
                        if !c.props.is_empty() {
                            it.documentation = Some(format!("Attributes: {}", c.props.join(", ")));
                        }
                        it
                    })
                    .collect();
                Some(Completions {
                    from: start,
                    to: end,
                    items,
                })
            }
            Context::Directive { start, prefix } => {
                let mut items = Vec::new();
                for d in &self.directives {
                    items.push(item(
                        format!("@{}", d.name),
                        "directive",
                        Some("custom directive".into()),
                    ));
                    if d.kind == "if" {
                        for extra in [
                            format!("else{}", d.name),
                            format!("unless{}", d.name),
                            format!("end{}", d.name),
                        ] {
                            items.push(item(
                                format!("@{extra}"),
                                "directive",
                                Some("custom if".into()),
                            ));
                        }
                    }
                }
                Some(Completions {
                    from: start,
                    to: start + prefix.len(),
                    items,
                })
            }
        }
    }

    /// Declarations of the symbol at a byte offset (empty when it is not a Laravel symbol).
    pub fn goto(&self, path: &str, src: &str, offset: usize) -> Vec<Location> {
        let Some(ctx) = context_at(src, offset) else {
            return Vec::new();
        };
        let imports = Imports::of(path, src);
        match ctx {
            Context::String(s) => {
                let Some(kind) = string_kind(&s) else {
                    return Vec::new();
                };
                let v = s.value.as_str();
                match kind {
                    StringKind::Route => {
                        let mut out: Vec<Location> = Vec::new();
                        for r in self.routes.iter().filter(|r| r.name.as_deref() == Some(v)) {
                            for loc in r.source.iter().chain(r.action_location.iter()) {
                                if !out.contains(loc) {
                                    out.push(loc.clone());
                                }
                            }
                        }
                        out
                    }
                    StringKind::View => resolve_view_path(&self.root, v)
                        .map(|p| vec![Location::file(&p)])
                        .unwrap_or_default(),
                    StringKind::Config => {
                        let exact = self.config.iter().find(|c| c.key == v);
                        // Fall back to the longest defined prefix (`app.providers.0`).
                        let entry = exact.or_else(|| {
                            self.config
                                .iter()
                                .filter(|c| v.starts_with(&format!("{}.", c.key)))
                                .max_by_key(|c| c.key.len())
                        });
                        entry.map(|c| vec![c.location.clone()]).unwrap_or_default()
                    }
                    StringKind::Env => self
                        .env
                        .iter()
                        .filter(|e| e.key == v)
                        .map(|e| e.location.clone())
                        .collect(),
                    StringKind::Translation => self
                        .translations
                        .iter()
                        .filter(|t| t.key == v)
                        .map(|t| t.location.clone())
                        .collect(),
                    StringKind::Column => s
                        .receiver
                        .as_deref()
                        .and_then(|r| self.receiver_model(src, &imports, path, r, offset))
                        .and_then(|m| m.attributes.iter().find(|a| a.name == v))
                        .and_then(|a| a.location.clone())
                        .into_iter()
                        .collect(),
                    StringKind::Relation => {
                        let name = v.split([':', '.']).next().unwrap_or(v);
                        s.receiver
                            .as_deref()
                            .and_then(|r| self.receiver_model(src, &imports, path, r, offset))
                            .and_then(|m| m.relations.iter().find(|r| r.name == name))
                            .map(|r| vec![r.location.clone()])
                            .unwrap_or_default()
                    }
                }
            }
            Context::ActionMethod(a) => {
                let class = imports.resolve(&a.class);
                crate::routes::action_location(&self.root, &self.psr4, &class, Some(&a.value))
                    .into_iter()
                    .collect()
            }
            Context::Property(p) => {
                let Some(m) = self.property_model(src, &imports, path, &p, offset) else {
                    return Vec::new();
                };
                let rest: String = src[offset..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                let name = format!("{}{rest}", p.prefix);
                if let Some(r) = m.relations.iter().find(|r| r.name == name) {
                    return vec![r.location.clone()];
                }
                m.attributes
                    .iter()
                    .find(|a| a.name == name)
                    .and_then(|a| a.location.clone())
                    .into_iter()
                    .collect()
            }
            Context::Static { class, prefix, .. } => {
                let rest: String = src[offset..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                let name = format!("{prefix}{rest}");
                self.model_by(&imports, &class)
                    .and_then(|m| m.scopes.iter().find(|s| s.name == name))
                    .map(|s| vec![s.location.clone()])
                    .unwrap_or_default()
            }
            Context::ComponentTag { value, .. } => self
                .components
                .iter()
                .filter(|c| c.tag == value)
                .flat_map(|c| {
                    let mut v: Vec<Location> = c.location().into_iter().collect();
                    if c.class_path.is_some() {
                        v.extend(c.view.as_deref().map(Location::file));
                    }
                    v
                })
                .collect(),
            Context::Directive { prefix, .. } => {
                let name = prefix.trim_start_matches('@');
                let rest: String = src[offset..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                let full = format!("{name}{rest}");
                self.directives
                    .iter()
                    .filter(|d| {
                        d.name == full
                            || (d.kind == "if"
                                && [
                                    format!("else{}", d.name),
                                    format!("unless{}", d.name),
                                    format!("end{}", d.name),
                                ]
                                .contains(&full))
                    })
                    .map(|d| d.location.clone())
                    .collect()
            }
        }
    }

    /// Gutter markers for a buffer.
    pub fn gutter(&self, path: &str, src: &str) -> Vec<GutterMark> {
        let mut marks = Vec::new();
        // Routed controller methods.
        if path.ends_with(".php") && !path.ends_with(".blade.php") && src.contains("class ") {
            let idx = index_source(path, src);
            for class in &idx.classes {
                for m in class
                    .members
                    .iter()
                    .filter(|m| m.kind == SymbolKind::Method)
                {
                    let routes: Vec<&crate::Route> = self
                        .routes
                        .iter()
                        .filter(|r| {
                            r.controller
                                .as_deref()
                                .is_some_and(|c| c.eq_ignore_ascii_case(&class.fqn))
                                && r.controller_method
                                    .as_deref()
                                    .unwrap_or("__invoke")
                                    .eq_ignore_ascii_case(&m.name)
                        })
                        .collect();
                    if routes.is_empty() {
                        continue;
                    }
                    let tooltip = routes
                        .iter()
                        .map(|r| {
                            let name = r
                                .name
                                .as_deref()
                                .map(|n| format!(" ({n})"))
                                .unwrap_or_default();
                            format!(
                                "{} /{}{name}",
                                r.method_label(),
                                r.uri.trim_start_matches('/')
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    marks.push(GutterMark {
                        line: crate::php::line_of(src, m.name_range.start),
                        kind: "route".into(),
                        tooltip,
                        targets: routes.iter().filter_map(|r| r.source.clone()).collect(),
                    });
                }
            }
        }
        // Route definitions → controller.
        if path.starts_with("routes/") {
            let mut seen = std::collections::HashSet::new();
            for r in self
                .routes
                .iter()
                .filter(|r| r.source.as_ref().is_some_and(|s| s.path == path))
            {
                let (Some(src_loc), Some(target)) = (&r.source, &r.action_location) else {
                    continue;
                };
                if !seen.insert((src_loc.line, target.clone())) {
                    continue;
                }
                let method = r.controller_method.as_deref().unwrap_or("__invoke");
                let class = r
                    .controller
                    .as_deref()
                    .map(|c| c.rsplit('\\').next().unwrap_or(c))
                    .unwrap_or("");
                marks.push(GutterMark {
                    line: src_loc.line,
                    kind: "controller".into(),
                    tooltip: format!("Navigate to {class}::{method}()"),
                    targets: vec![target.clone()],
                });
            }
        }
        // view('name') calls and Blade includes.
        let bytes = src.as_bytes();
        for needle in [
            "view(",
            "View::make(",
            "@include(",
            "@extends(",
            "@includeIf(",
            "@each(",
            "->markdown(",
        ] {
            let mut from = 0;
            while let Some(i) = src[from..].find(needle) {
                let at = from + i;
                from = at + needle.len();
                if needle == "view("
                    && at > 0
                    && (bytes[at - 1].is_ascii_alphanumeric()
                        || bytes[at - 1] == b'_'
                        || bytes[at - 1] == b':')
                {
                    continue;
                }
                let rest = src[from..].trim_start();
                let Some(q) = rest.chars().next().filter(|c| *c == '\'' || *c == '"') else {
                    continue;
                };
                let Some(end) = rest[1..].find(q) else {
                    continue;
                };
                let name = &rest[1..1 + end];
                let Some(target) = resolve_view_path(&self.root, name) else {
                    continue;
                };
                let line = crate::php::line_of(src, at);
                if marks
                    .iter()
                    .any(|m: &GutterMark| m.line == line && m.kind == "view")
                {
                    continue;
                }
                marks.push(GutterMark {
                    line,
                    kind: "view".into(),
                    tooltip: format!("Navigate to view {name}"),
                    targets: vec![Location::file(&target)],
                });
            }
        }
        marks.sort_by_key(|m| m.line);
        marks
    }
}

/// `\Illuminate\Support\Carbon` → `Carbon`.
fn short_type(ty: &str) -> String {
    ty.split('|')
        .map(|p| p.trim_start_matches('\\').rsplit('\\').next().unwrap_or(p))
        .collect::<Vec<_>>()
        .join("|")
}
