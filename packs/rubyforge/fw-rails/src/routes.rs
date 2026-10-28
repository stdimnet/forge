//! `config/routes.rb` read statically, like `bin/rails routes` prints it: `resources` /
//! `resource` (with `only`, `except`, `controller`, `path`, `as`, `param`, nested
//! resources, `member` / `collection` blocks and `on:`), `namespace`, `scope` (path,
//! `module`, `as`, `controller`), verbs (`get`, `post`, `patch`, `put`, `delete`, `match`
//! with `via:`), `root`, `mount`, `concern` / `concerns` and `draw(:file)` for
//! `config/routes/<file>.rb`. [`parse_rails_routes`] reads the output of `bin/rails routes`
//! (trusted projects only), which also knows the routes of engines and gems.

use serde::{Deserialize, Serialize};

use crate::inflect::{pluralize, singularize, underscore};
use crate::source::{call_args, logical_lines, tokens, Args, LineIndex, Logical, Tok, Val};
use crate::Location;

/// One route: what `bin/rails routes` prints in a row.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    /// HTTP methods; empty for any (`match … via: :all`, `mount`).
    pub methods: Vec<String>,
    /// `/articles/:id`, without `(.:format)`.
    pub path: String,
    /// The route's name, the prefix of its helpers (`article` → `article_path`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `admin/users`; the application's class for mounts (`Sidekiq::Web`).
    pub controller: String,
    /// `index`; empty for mounts and redirects.
    pub action: String,
    /// `route`, `mount` or `redirect`.
    pub kind: String,
    /// The call in `config/routes.rb` (or a drawn file).
    pub location: Location,
    /// The controller's `def <action>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_location: Option<Location>,
    /// The path with sample values (`/articles/1`), for the HTTP client.
    pub sample_path: String,
}

impl Route {
    /// `articles#index`, `Sidekiq::Web`, `redirect`.
    pub fn handler(&self) -> String {
        match self.kind.as_str() {
            "mount" => self.controller.clone(),
            "redirect" => format!("redirect({})", self.action),
            _ => format!("{}#{}", self.controller, self.action),
        }
    }
}

/// Replace dynamic segments with sample values: `/articles/:article_id/comments(.:format)`
/// → `/articles/1/comments`.
pub fn sample_path(path: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ if depth > 0 => {}
            ':' | '*' => {
                let mut name = String::new();
                while let Some(&n) = chars.peek() {
                    if n.is_alphanumeric() || n == '_' {
                        name.push(n);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if c == ':' && (name == "id" || name.ends_with("_id")) {
                    out.push('1');
                } else {
                    out.push_str(&name);
                }
            }
            _ => out.push(c),
        }
    }
    while out.contains("//") {
        out = out.replace("//", "/");
    }
    if out.len() > 1 {
        out = out.trim_end_matches('/').to_string();
    }
    if out.is_empty() {
        "/".into()
    } else {
        out
    }
}

fn join_path(base: &str, seg: &str) -> String {
    let seg = seg.trim_matches('/');
    if seg.is_empty() {
        return base.to_string();
    }
    format!("{}/{seg}", base.trim_end_matches('/'))
}

fn join_name(parts: &[String]) -> Option<String> {
    let parts: Vec<&str> = parts
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join("_"))
}

fn module_join(module: &str, controller: &str) -> String {
    if let Some(abs) = controller.strip_prefix('/') {
        return abs.to_string();
    }
    if module.is_empty() {
        controller.to_string()
    } else {
        format!("{module}/{controller}")
    }
}

#[derive(Debug, Clone)]
struct Res {
    /// The name as written: `articles`, `profile`.
    name: String,
    singular: bool,
    controller: String,
    seg: String,
    param: String,
    /// The name base (`as:` or the name).
    as_name: String,
    only: Option<Vec<String>>,
    except: Vec<String>,
    location: Location,
    /// The context the resource itself lives in.
    base: Ctx,
}

impl Res {
    fn member_name(&self) -> String {
        if self.singular {
            self.as_name.clone()
        } else {
            singularize(&self.as_name)
        }
    }
    fn collection_name(&self) -> String {
        if !self.singular && singularize(&self.as_name) == self.as_name {
            format!("{}_index", self.as_name)
        } else {
            self.as_name.clone()
        }
    }
    fn wants(&self, action: &str) -> bool {
        self.only
            .as_ref()
            .is_none_or(|o| o.iter().any(|a| a == action))
            && !self.except.iter().any(|a| a == action)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Nested,
    Member,
    Collection,
}

#[derive(Debug, Clone)]
enum Frame {
    Plain,
    Scope {
        path: String,
        module: String,
        name: String,
        controller: Option<String>,
    },
    Res(Box<Res>),
    Member,
    Collection,
}

#[derive(Debug, Clone, Default)]
struct Ctx {
    path: String,
    module: String,
    names: Vec<String>,
    controller: Option<String>,
    res: Option<(Box<Res>, ModeBox)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ModeBox(Mode);

impl Default for ModeBox {
    fn default() -> Self {
        ModeBox(Mode::Nested)
    }
}

fn nested(c: &mut Ctx, r: &Res) {
    c.path = join_path(&c.path, &r.seg);
    if !r.singular {
        c.path = format!("{}/:{}_{}", c.path, singularize(&r.name), r.param);
    }
    c.names.push(r.member_name());
    c.controller = None;
    c.res = Some((Box::new(r.clone()), ModeBox(Mode::Nested)));
}

fn context(stack: &[Frame]) -> Ctx {
    let mut c = Ctx::default();
    let mut pending: Option<&Res> = None;
    for f in stack {
        if let Some(r) = pending.take() {
            match f {
                Frame::Member => {
                    c.path = join_path(&c.path, &r.seg);
                    if !r.singular {
                        c.path = format!("{}/:{}", c.path, r.param);
                    }
                    c.res = Some((Box::new(r.clone()), ModeBox(Mode::Member)));
                    continue;
                }
                Frame::Collection => {
                    c.path = join_path(&c.path, &r.seg);
                    c.res = Some((Box::new(r.clone()), ModeBox(Mode::Collection)));
                    continue;
                }
                _ => nested(&mut c, r),
            }
        }
        match f {
            Frame::Scope {
                path,
                module,
                name,
                controller,
            } => {
                c.path = join_path(&c.path, path);
                if !module.is_empty() {
                    c.module = module_join(&c.module, module);
                }
                if !name.is_empty() {
                    c.names.push(name.clone());
                }
                if controller.is_some() {
                    c.controller = controller.clone();
                }
                c.res = None;
            }
            Frame::Res(r) => pending = Some(r),
            _ => {}
        }
    }
    if let Some(r) = pending {
        nested(&mut c, r);
    }
    c
}

/// Routes drawn by a routes file, with the files `draw` loads read through `read`.
pub fn parse_routes(path: &str, text: &str, read: &dyn Fn(&str) -> Option<String>) -> Vec<Route> {
    let mut p = RouteParser {
        read,
        out: Vec::new(),
        stack: Vec::new(),
        concerns: Vec::new(),
        capture: None,
        depth: 0,
    };
    p.file(path, text);
    while let Some(f) = p.stack.pop() {
        p.close(f);
    }
    p.out
}

/// A logical line with its file and position, kept to replay a `concern`.
type Captured = (String, Logical, u32, u32);

struct Capture {
    name: String,
    depth: usize,
    lines: Vec<Captured>,
}

struct RouteParser<'a> {
    read: &'a dyn Fn(&str) -> Option<String>,
    out: Vec<Route>,
    stack: Vec<Frame>,
    concerns: Vec<(String, Vec<Captured>)>,
    capture: Option<Capture>,
    /// `draw` nesting, against include loops.
    depth: usize,
}

/// Whether a logical line opens a block that `end` closes (and isn't a route call).
fn opens(toks: &[crate::source::Token]) -> bool {
    let Some(first) = toks.first().and_then(|t| t.ident()) else {
        return false;
    };
    matches!(
        first,
        "if" | "unless" | "case" | "begin" | "while" | "until" | "def" | "class" | "module" | "for"
    ) || toks.iter().any(|t| t.is_ident("do"))
}

impl RouteParser<'_> {
    fn file(&mut self, path: &str, text: &str) {
        let idx = LineIndex::new(text);
        for l in logical_lines(text) {
            let (line, col) = idx.position(l.start);
            self.line(path, &l, line, col);
        }
    }

    fn line(&mut self, path: &str, l: &Logical, line: u32, col: u32) {
        let toks = tokens(&l.text);
        let Some(first) = toks.first() else { return };
        let is_end = first.is_ident("end") || first.is_punct('}');
        if let Some(cap) = &mut self.capture {
            if is_end {
                if cap.depth == 0 {
                    let cap = self.capture.take().expect("capture");
                    self.concerns.push((cap.name, cap.lines));
                    return;
                }
                cap.depth -= 1;
            } else if opens(&toks) {
                cap.depth += 1;
            }
            cap.lines.push((path.to_string(), l.clone(), line, col));
            return;
        }
        if is_end {
            if let Some(f) = self.stack.pop() {
                self.close(f);
            }
            return;
        }
        let loc = Location {
            path: path.to_string(),
            line,
            character: col,
        };
        let name = match &first.tok {
            Tok::Ident(n) => n.as_str(),
            _ => {
                if opens(&toks) {
                    self.stack.push(Frame::Plain);
                }
                return;
            }
        };
        // `Rails.application.routes.draw do` starts with a constant: handled above.
        let args = call_args(&toks);
        match name {
            "resources" | "resource" => self.resources(name == "resource", &args, &loc),
            "namespace" => {
                let n = args
                    .positional
                    .first()
                    .and_then(Val::text)
                    .unwrap_or("")
                    .to_string();
                let frame = Frame::Scope {
                    path: args.text("path").unwrap_or(&n).to_string(),
                    module: args.text("module").unwrap_or(&n).to_string(),
                    name: args.text("as").unwrap_or(&n).to_string(),
                    controller: None,
                };
                self.block(frame, args.block);
            }
            "scope" => {
                let path = args
                    .text("path")
                    .or_else(|| args.positional.first().and_then(Val::text))
                    .unwrap_or("")
                    .to_string();
                let frame = Frame::Scope {
                    path,
                    module: args.text("module").unwrap_or("").to_string(),
                    name: args.text("as").unwrap_or("").to_string(),
                    controller: args.text("controller").map(str::to_string),
                };
                self.block(frame, args.block);
            }
            "controller" => {
                let frame = Frame::Scope {
                    path: String::new(),
                    module: String::new(),
                    name: String::new(),
                    controller: args
                        .positional
                        .first()
                        .and_then(Val::text)
                        .map(str::to_string),
                };
                self.block(frame, args.block);
            }
            "member" if args.block => self.stack.push(Frame::Member),
            "collection" if args.block => self.stack.push(Frame::Collection),
            "concern" if args.block => {
                self.capture = Some(Capture {
                    name: args
                        .positional
                        .first()
                        .and_then(Val::text)
                        .unwrap_or("")
                        .to_string(),
                    depth: 0,
                    lines: Vec::new(),
                });
            }
            "concerns" => {
                let names: Vec<String> = args.positional.iter().flat_map(Val::texts).collect();
                self.replay(&names);
            }
            "draw" => {
                let file = args.positional.first().and_then(Val::text).unwrap_or("");
                let path = format!("config/routes/{file}.rb");
                if self.depth < 4 {
                    if let Some(text) = (self.read)(&path) {
                        self.depth += 1;
                        self.file(&path, &text);
                        self.depth -= 1;
                    }
                }
            }
            "root" => self.root(&args, &loc),
            "mount" => self.mount(&l.text, &args, &loc),
            "get" | "post" | "patch" | "put" | "delete" | "options" | "head" | "match" => {
                self.verb(name, &args, &loc);
                if args.block {
                    self.stack.push(Frame::Plain);
                }
            }
            _ => {
                if opens(&toks) {
                    self.stack.push(Frame::Plain);
                }
            }
        }
    }

    fn block(&mut self, frame: Frame, block: bool) {
        if block {
            self.stack.push(frame);
        }
    }

    fn replay(&mut self, names: &[String]) {
        let lines: Vec<Captured> = names
            .iter()
            .filter_map(|n| self.concerns.iter().find(|(c, _)| c == n))
            .flat_map(|(_, l)| l.clone())
            .collect();
        for (path, l, line, col) in lines {
            self.line(&path, &l, line, col);
        }
    }

    fn resources(&mut self, singular: bool, args: &Args, loc: &Location) {
        let names: Vec<String> = args
            .positional
            .iter()
            .filter_map(|v| match v {
                Val::Sym(s) | Val::Str(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        let concerns: Vec<String> = args.get("concerns").map(Val::texts).unwrap_or_default();
        let count = names.len();
        for (i, name) in names.into_iter().enumerate() {
            let base = context(&self.stack);
            let module = base.module.clone();
            let default_controller = if singular {
                pluralize(&name)
            } else {
                name.clone()
            };
            let controller = module_join(
                &module,
                args.text("controller").unwrap_or(&default_controller),
            );
            let res = Res {
                seg: args.text("path").unwrap_or(&name).to_string(),
                param: args.text("param").unwrap_or("id").to_string(),
                as_name: args.text("as").unwrap_or(&name).to_string(),
                only: args.get("only").map(Val::texts),
                except: args.get("except").map(Val::texts).unwrap_or_default(),
                location: loc.clone(),
                controller,
                singular,
                name,
                base,
            };
            self.stack.push(Frame::Res(Box::new(res)));
            if !concerns.is_empty() {
                self.replay(&concerns);
            }
            // A block belongs to the last name; the others close at once.
            if !(args.block && i + 1 == count) {
                if let Some(f) = self.stack.pop() {
                    self.close(f);
                }
            }
        }
    }

    fn close(&mut self, f: Frame) {
        let Frame::Res(r) = f else { return };
        let c = &r.base;
        let route = |methods: &[&str], path: String, name: Option<String>, action: &str| Route {
            methods: methods.iter().map(|m| m.to_string()).collect(),
            sample_path: sample_path(&path),
            path,
            name,
            controller: r.controller.clone(),
            action: action.into(),
            kind: "route".into(),
            location: r.location.clone(),
            action_location: None,
        };
        let mut names = c.names.clone();
        let mut out = Vec::new();
        if r.singular {
            let path = join_path(&c.path, &r.seg);
            names.push(r.as_name.clone());
            let base = join_name(&names);
            let prefixed = |p: &str| base.as_ref().map(|b| format!("{p}_{b}"));
            for (action, methods, p, name) in [
                (
                    "new",
                    &["GET"][..],
                    join_path(&path, "new"),
                    prefixed("new"),
                ),
                ("edit", &["GET"], join_path(&path, "edit"), prefixed("edit")),
                ("show", &["GET"], path.clone(), base.clone()),
                ("update", &["PATCH", "PUT"], path.clone(), None),
                ("destroy", &["DELETE"], path.clone(), None),
                ("create", &["POST"], path.clone(), None),
            ] {
                out.push(route(methods, p, name, action));
            }
        } else {
            let coll = join_path(&c.path, &r.seg);
            let member = format!("{coll}/:{}", r.param);
            let mut cn = names.clone();
            cn.push(r.collection_name());
            names.push(r.member_name());
            let coll_name = join_name(&cn);
            let member_name = join_name(&names);
            let prefixed = |p: &str| member_name.as_ref().map(|b| format!("{p}_{b}"));
            for (action, methods, p, name) in [
                ("index", &["GET"][..], coll.clone(), coll_name.clone()),
                ("create", &["POST"], coll.clone(), None),
                ("new", &["GET"], join_path(&coll, "new"), prefixed("new")),
                (
                    "edit",
                    &["GET"],
                    join_path(&member, "edit"),
                    prefixed("edit"),
                ),
                ("show", &["GET"], member.clone(), member_name.clone()),
                ("update", &["PATCH", "PUT"], member.clone(), None),
                ("destroy", &["DELETE"], member.clone(), None),
            ] {
                out.push(route(methods, p, name, action));
            }
        }
        // Like `rails routes`, a path's name goes to its first route (`create` has the
        // collection's name without `index`, `update` the member's without `show`).
        let named: Vec<(String, String)> = out
            .iter()
            .filter_map(|x| x.name.clone().map(|n| (x.path.clone(), n)))
            .collect();
        out.retain(|x| r.wants(&x.action));
        let mut seen: Vec<String> = Vec::new();
        for x in &mut out {
            if seen.contains(&x.path) {
                x.name = None;
                continue;
            }
            seen.push(x.path.clone());
            if x.name.is_none() {
                x.name = named
                    .iter()
                    .find(|(p, _)| *p == x.path)
                    .map(|(_, n)| n.clone());
            }
        }
        self.out.extend(out);
    }

    fn root(&mut self, args: &Args, loc: &Location) {
        let c = context(&self.stack);
        let target = args
            .text("to")
            .or_else(|| args.positional.first().and_then(Val::text))
            .unwrap_or("");
        let (controller, action) = match target.split_once('#') {
            Some((ctl, act)) => (ctl.to_string(), act.to_string()),
            None => (
                args.text("controller").unwrap_or(target).to_string(),
                args.text("action").unwrap_or("index").to_string(),
            ),
        };
        let mut names = c.names.clone();
        names.push("root".into());
        let path = if c.path.is_empty() {
            "/".into()
        } else {
            c.path.clone()
        };
        self.out.push(Route {
            methods: vec!["GET".into()],
            sample_path: sample_path(&path),
            path,
            name: join_name(&names),
            controller: module_join(&c.module, &controller),
            action,
            kind: "route".into(),
            location: loc.clone(),
            action_location: None,
        });
    }

    fn mount(&mut self, text: &str, args: &Args, loc: &Location) {
        let c = context(&self.stack);
        let rest = text.trim_start_matches("mount").trim();
        let (app, at) = match rest.split_once("=>") {
            Some((app, path)) => {
                let path = tokens(path)
                    .into_iter()
                    .find_map(|t| match t.tok {
                        Tok::Str { value, .. } => Some(value),
                        _ => None,
                    })
                    .unwrap_or_default();
                (app.trim().trim_start_matches('(').trim().to_string(), path)
            }
            None => (
                rest.split(',')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_start_matches('(')
                    .to_string(),
                args.text("at").unwrap_or("").to_string(),
            ),
        };
        if app.is_empty() {
            return;
        }
        let path = join_path(&c.path, &at);
        let path = if path.is_empty() { "/".into() } else { path };
        let mut names = c.names.clone();
        names.push(match args.text("as") {
            Some(a) => a.to_string(),
            None => underscore(app.split('.').next().unwrap_or(&app)).replace('/', "_"),
        });
        self.out.push(Route {
            methods: Vec::new(),
            sample_path: sample_path(&path),
            path,
            name: join_name(&names),
            controller: app,
            action: String::new(),
            kind: "mount".into(),
            location: loc.clone(),
            action_location: None,
        });
    }

    fn verb(&mut self, verb: &str, args: &Args, loc: &Location) {
        let mut stack = self.stack.clone();
        // `get :preview, on: :member` acts like a `member do` block.
        match args.text("on") {
            Some("member") => stack.push(Frame::Member),
            Some("collection") => stack.push(Frame::Collection),
            _ => {}
        }
        let c = context(&stack);
        let methods: Vec<String> = if verb == "match" {
            let via = args.get("via").map(Val::texts).unwrap_or_default();
            if via.iter().any(|v| v == "all") {
                Vec::new()
            } else {
                via.iter().map(|v| v.to_uppercase()).collect()
            }
        } else {
            vec![verb.to_uppercase()]
        };
        // The path and the target: `get "x", to: "a#b"`, `get "x" => "a#b"`, `get :x`.
        let (path_arg, mut target, symbol) = match args.pair() {
            Some((p, t)) => (
                p.text().unwrap_or("").to_string(),
                match t {
                    Val::Sym(a) => Some(format!("#{a}")),
                    Val::Redirect(to) => Some(format!("redirect:{to}")),
                    v => v.text().map(str::to_string),
                },
                false,
            ),
            None => {
                let first = args.positional.first();
                (
                    first.and_then(Val::text).unwrap_or("").to_string(),
                    None,
                    matches!(first, Some(Val::Sym(_))),
                )
            }
        };
        if target.is_none() {
            target = match args.get("to") {
                Some(Val::Redirect(to)) => Some(format!("redirect:{to}")),
                Some(v) => v.text().map(str::to_string),
                None => None,
            };
        }
        let path = join_path(&c.path, &path_arg);
        let path = if path.is_empty() {
            "/".to_string()
        } else {
            path
        };
        let mode = c.res.as_ref().map(|(_, m)| m.0);
        let res_controller = c.res.as_ref().map(|(r, _)| r.controller.clone());
        let simple = !path_arg.contains([':', '*', '(']);
        let seg_action = path_arg
            .trim_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
            .replace('-', "_");
        let (kind, controller, action) = match target.as_deref() {
            Some(t) if t.starts_with("redirect:") => (
                "redirect",
                String::new(),
                t["redirect:".len()..].to_string(),
            ),
            Some(t) if t.contains('#') => {
                let (ctl, act) = t.split_once('#').unwrap_or((t, ""));
                let controller = if ctl.is_empty() {
                    c.controller
                        .clone()
                        .map(|x| module_join(&c.module, &x))
                        .or(res_controller.clone())
                        .unwrap_or_default()
                } else {
                    module_join(&c.module, ctl)
                };
                ("route", controller, act.to_string())
            }
            Some(t) if t.chars().next().is_some_and(char::is_uppercase) => {
                ("mount", t.to_string(), String::new())
            }
            Some(t) => (
                "route",
                module_join(&c.module, t),
                args.text("action").unwrap_or(&seg_action).to_string(),
            ),
            None => {
                let action = args.text("action").map(str::to_string);
                if let Some(ctl) = args.text("controller") {
                    (
                        "route",
                        module_join(&c.module, ctl),
                        action.unwrap_or(seg_action.clone()),
                    )
                } else if let Some(ctl) = c.controller.clone().map(|x| module_join(&c.module, &x)) {
                    ("route", ctl, action.unwrap_or(seg_action.clone()))
                } else if let Some(ctl) = res_controller.clone() {
                    ("route", ctl, action.unwrap_or(seg_action.clone()))
                } else {
                    // `get "photos/search"` → photos#search.
                    let segs: Vec<&str> = path_arg
                        .trim_matches('/')
                        .split('/')
                        .filter(|s| !s.starts_with([':', '*', '(']))
                        .collect();
                    if segs.len() >= 2 {
                        let (act, ctl) = segs.split_last().expect("segments");
                        (
                            "route",
                            module_join(&c.module, &ctl.join("/")),
                            act.replace('-', "_"),
                        )
                    } else {
                        ("route", String::new(), action.unwrap_or(seg_action.clone()))
                    }
                }
            }
        };
        let as_name = args.text("as").map(str::to_string);
        let derived = || -> Option<String> {
            if !simple || path_arg.trim_matches('/').is_empty() {
                return None;
            }
            Some(path_arg.trim_matches('/').replace(['/', '-', '.'], "_"))
        };
        let name = match (mode, &c.res) {
            (Some(Mode::Member), Some((r, _))) => {
                let mut n = vec![as_name.clone().or(derived()).unwrap_or(action.clone())];
                n.extend(r.base.names.clone());
                n.push(r.member_name());
                if symbol || as_name.is_some() || simple {
                    join_name(&n)
                } else {
                    None
                }
            }
            (Some(Mode::Collection), Some((r, _))) => {
                let mut n = vec![as_name.clone().or(derived()).unwrap_or(action.clone())];
                n.extend(r.base.names.clone());
                n.push(r.collection_name());
                if symbol || as_name.is_some() || simple {
                    join_name(&n)
                } else {
                    None
                }
            }
            _ => match as_name.or_else(derived) {
                Some(n) => {
                    let mut all = c.names.clone();
                    all.push(n);
                    join_name(&all)
                }
                None => None,
            },
        };
        self.out.push(Route {
            methods,
            sample_path: sample_path(&path),
            path,
            name,
            controller,
            action,
            kind: kind.into(),
            location: loc.clone(),
            action_location: None,
        });
    }
}

/// Routes from the output of `bin/rails routes` (the default table format). Each row's
/// place in `config/routes.rb` is taken from `known` (the static routes) when one matches.
pub fn parse_rails_routes(output: &str, known: &[Route]) -> Vec<Route> {
    let fallback = Location {
        path: "config/routes.rb".into(),
        line: 0,
        character: 0,
    };
    let mut out: Vec<Route> = Vec::new();
    for line in output.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some(ui) = words.iter().position(|w| w.starts_with('/')) else {
            continue;
        };
        if words.first() == Some(&"Prefix") {
            continue;
        }
        let before = &words[..ui];
        let is_verb = |w: &str| {
            !w.is_empty()
                && w.split('|').all(|v| {
                    matches!(
                        v,
                        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS" | "HEAD"
                    )
                })
        };
        let (name, verb) = match before {
            [n, v] if is_verb(v) => (Some(n.to_string()), Some(*v)),
            [v] if is_verb(v) => (None, Some(*v)),
            [n] => (Some(n.to_string()), None),
            _ => (None, None),
        };
        let path = words[ui].trim_end_matches("(.:format)").to_string();
        let target = words.get(ui + 1).copied().unwrap_or("");
        let (kind, controller, action) = match target.split_once('#') {
            Some((c, a)) => ("route", c.to_string(), a.to_string()),
            None if target.starts_with("redirect") => {
                ("redirect", String::new(), target.to_string())
            }
            None => ("mount", target.to_string(), String::new()),
        };
        let methods: Vec<String> = verb
            .map(|v| v.split('|').map(str::to_string).collect())
            .unwrap_or_default();
        let location = known
            .iter()
            .find(|r| r.path == path && r.controller == controller && r.action == action)
            .or_else(|| known.iter().find(|r| r.name.is_some() && r.name == name))
            .map(|r| r.location.clone())
            .unwrap_or_else(|| fallback.clone());
        // `PATCH` and `PUT` rows of the same action merge into one route.
        if let Some(prev) = out.last_mut() {
            if prev.path == path
                && prev.controller == controller
                && prev.action == action
                && name.is_none()
            {
                prev.methods.extend(methods);
                continue;
            }
        }
        out.push(Route {
            methods,
            sample_path: sample_path(&path),
            path,
            name,
            controller,
            action,
            kind: kind.into(),
            location,
            action_location: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(src: &str) -> Vec<String> {
        parse_routes("config/routes.rb", src, &|_| None)
            .iter()
            .map(|r| {
                format!(
                    "{} {} {} {}",
                    r.name.clone().unwrap_or_else(|| "-".into()),
                    if r.methods.is_empty() {
                        "ANY".into()
                    } else {
                        r.methods.join("|")
                    },
                    r.path,
                    r.handler()
                )
            })
            .collect()
    }

    #[test]
    fn resources_with_nesting_member_and_collection() {
        let got = rows(
            r#"Rails.application.routes.draw do
  root "articles#index"
  resources :articles do
    resources :comments, only: [:create, :destroy]
    member do
      post :publish
    end
    collection do
      get :search
    end
    get :preview, on: :member
  end
  resource :profile, only: %i[show edit update]
end
"#,
        );
        assert_eq!(
            got,
            vec![
                "root GET / articles#index",
                "article_comments POST /articles/:article_id/comments comments#create",
                "article_comment DELETE /articles/:article_id/comments/:id comments#destroy",
                "publish_article POST /articles/:id/publish articles#publish",
                "search_articles GET /articles/search articles#search",
                "preview_article GET /articles/:id/preview articles#preview",
                "articles GET /articles articles#index",
                "- POST /articles articles#create",
                "new_article GET /articles/new articles#new",
                "edit_article GET /articles/:id/edit articles#edit",
                "article GET /articles/:id articles#show",
                "- PATCH|PUT /articles/:id articles#update",
                "- DELETE /articles/:id articles#destroy",
                "edit_profile GET /profile/edit profiles#edit",
                "profile GET /profile profiles#show",
                "- PATCH|PUT /profile profiles#update",
            ]
        );
    }

    #[test]
    fn namespaces_scopes_and_verbs() {
        let got = rows(
            r#"Rails.application.routes.draw do
  get "up" => "rails/health#show", as: :rails_health_check
  get "about", to: "pages#about"
  get "photos/search"
  match "contact", to: "pages#contact", via: [:get, :post]
  get "/stories", to: redirect("/articles")
  namespace :admin do
    root to: "dashboard#show"
    resources :users, except: :destroy, param: :slug
    get "stats", to: "stats#index", as: :statistics
  end
  scope module: :api, path: "/api/v1", as: :api do
    resources :posts, only: :index
  end
  scope controller: :sessions do
    get "login", action: :new
    delete "logout" => :destroy
  end
  mount ActionCable.server => "/cable"
  mount Sidekiq::Web, at: "/sidekiq"
  if Rails.env.development?
    get "debug", to: "debug#show"
  end
end
"#,
        );
        assert_eq!(
            got,
            vec![
                "rails_health_check GET /up rails/health#show",
                "about GET /about pages#about",
                "photos_search GET /photos/search photos#search",
                "contact GET|POST /contact pages#contact",
                "stories GET /stories redirect(/articles)",
                "admin_root GET /admin admin/dashboard#show",
                "admin_users GET /admin/users admin/users#index",
                "- POST /admin/users admin/users#create",
                "new_admin_user GET /admin/users/new admin/users#new",
                "edit_admin_user GET /admin/users/:slug/edit admin/users#edit",
                "admin_user GET /admin/users/:slug admin/users#show",
                "- PATCH|PUT /admin/users/:slug admin/users#update",
                "admin_statistics GET /admin/stats admin/stats#index",
                "api_posts GET /api/v1/posts api/posts#index",
                "login GET /login sessions#new",
                "logout DELETE /logout sessions#destroy",
                "action_cable ANY /cable ActionCable.server",
                "sidekiq_web ANY /sidekiq Sidekiq::Web",
                "debug GET /debug debug#show",
            ]
        );
    }

    #[test]
    fn concerns_and_drawn_files() {
        let read = |p: &str| {
            (p == "config/routes/admin.rb").then(|| {
                "namespace :admin do\n  resources :reports, only: :index\nend\n".to_string()
            })
        };
        let routes = parse_routes(
            "config/routes.rb",
            "Rails.application.routes.draw do\n  concern :commentable do\n    resources :comments, only: :index\n  end\n  resources :photos, only: :show, concerns: :commentable\n  draw :admin\nend\n",
            &read,
        );
        let got: Vec<String> = routes
            .iter()
            .map(|r| {
                format!(
                    "{} {} {}",
                    r.name.clone().unwrap_or_default(),
                    r.path,
                    r.location.path
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![
                "photo_comments /photos/:photo_id/comments config/routes.rb",
                "photo /photos/:id config/routes.rb",
                "admin_reports /admin/reports config/routes/admin.rb",
            ]
        );
        assert_eq!(routes[2].location.line, 1);
        assert_eq!(routes[0].sample_path, "/photos/1/comments");
    }

    #[test]
    fn rails_routes_output() {
        let known = parse_routes(
            "config/routes.rb",
            "Rails.application.routes.draw do\n  resources :articles\nend\n",
            &|_| None,
        );
        let out =
            "                  Prefix Verb   URI Pattern                  Controller#Action\n\
                         articles GET    /articles(.:format)          articles#index\n\
                                  POST   /articles(.:format)          articles#create\n\
                          article PATCH  /articles/:id(.:format)      articles#update\n\
                                  PUT    /articles/:id(.:format)      articles#update\n\
               rails_health_check GET    /up(.:format)                rails/health#show\n\
                      sidekiq_web        /sidekiq                     Sidekiq::Web\n";
        let r = parse_rails_routes(out, &known);
        assert_eq!(r.len(), 5);
        assert_eq!(r[0].location.line, 1);
        assert_eq!(r[2].methods, ["PATCH", "PUT"]);
        assert_eq!(r[3].location.line, 0);
        assert_eq!(
            (r[4].kind.as_str(), r[4].name.as_deref()),
            ("mount", Some("sidekiq_web"))
        );
        assert_eq!(
            sample_path("/(:locale)/articles/:slug/*rest"),
            "/articles/slug/rest"
        );
    }
}
