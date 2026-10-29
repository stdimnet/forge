//! FastAPI path operations: `@app.get("/p")` … `@router.api_route(..., methods=[...])`,
//! `@app.websocket("/ws")`, on `FastAPI()` apps and `APIRouter(prefix=...)` routers, with
//! the prefixes of `include_router(router, prefix=...)` (nested routers too).

use std::collections::{BTreeMap, BTreeSet};

use crate::source::{self, Line, Token};
use crate::{sample_path, Project, Route};

const VERBS: [&str; 8] = [
    "get", "post", "put", "patch", "delete", "options", "head", "trace",
];
const MAX_NESTING: usize = 6;

type Key = (String, String);

#[derive(Debug, Clone, Default)]
struct Object {
    app: bool,
    prefix: String,
    /// Where it is included: (parent, prefix of the `include_router` call).
    included: Vec<(Key, String)>,
}

fn objects(project: &Project) -> BTreeMap<Key, Object> {
    let mut out: BTreeMap<Key, Object> = BTreeMap::new();
    for (path, _) in project.python() {
        for (var, line, open) in project.assigned_calls(path, &["FastAPI", "APIRouter"]) {
            let callee = &line.tokens[open - 1].text;
            let prefix = source::call_args(&line.tokens, open)
                .and_then(|(args, _)| source::keyword(&args, "prefix").and_then(|a| a.string()))
                .unwrap_or_default();
            out.insert(
                (path.to_string(), var.to_string()),
                Object {
                    app: callee == "FastAPI",
                    prefix,
                    included: Vec::new(),
                },
            );
        }
    }
    for (path, lines) in project.python() {
        for l in lines {
            let t = &l.tokens;
            for (i, tok) in t.iter().enumerate() {
                if !tok.is_name("include_router")
                    || i < 2
                    || !t[i - 1].is_op(".")
                    || !t.get(i + 1).is_some_and(|x| x.is_op("("))
                {
                    continue;
                }
                let Some((parent, _)) = source::dotted_before(t, i - 1) else {
                    continue;
                };
                let Some((args, _)) = source::call_args(t, i + 1) else {
                    continue;
                };
                let Some(child) = source::positional(&args, 0).and_then(|a| a.dotted()) else {
                    continue;
                };
                let (Some(pk), Some(ck)) = (
                    project.resolve_variable(path, &parent),
                    project.resolve_variable(path, &child),
                ) else {
                    continue;
                };
                let prefix = source::keyword(&args, "prefix")
                    .and_then(|a| a.string())
                    .unwrap_or_default();
                if let Some(o) = out.get_mut(&ck) {
                    o.included.push((pk, prefix));
                }
            }
        }
    }
    out
}

/// Every prefix an object is served under.
fn mounts(
    objects: &BTreeMap<Key, Object>,
    key: &Key,
    depth: usize,
    seen: &mut BTreeSet<Key>,
) -> Vec<String> {
    let Some(o) = objects.get(key) else {
        return vec![String::new()];
    };
    if o.app {
        return vec![o.prefix.clone()];
    }
    if depth > MAX_NESTING || !seen.insert(key.clone()) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (parent, prefix) in &o.included {
        for p in mounts(objects, parent, depth + 1, seen) {
            out.push(format!("{p}{prefix}{}", o.prefix));
        }
    }
    seen.remove(key);
    if out.is_empty() && o.included.is_empty() {
        // A router nobody includes (yet): show it with its own prefix.
        out.push(o.prefix.clone());
    }
    out
}

/// The path operations of every app and router.
pub fn routes(project: &Project) -> Vec<Route> {
    let objects = objects(project);
    let mut out = Vec::new();
    for (path, lines) in project.python() {
        for (i, l) in lines.iter().enumerate() {
            let Some(("def", func)) = source::definition(l) else {
                continue;
            };
            for d in source::decorators(lines, i) {
                operation(project, &objects, path, d, func, &mut out);
            }
        }
    }
    out
}

fn operation(
    project: &Project,
    objects: &BTreeMap<Key, Object>,
    path: &str,
    d: &Line,
    func: &Token,
    out: &mut Vec<Route>,
) {
    let t = &d.tokens;
    let Some(open) = t.iter().position(|x| x.is_op("(")) else {
        return;
    };
    let Some(callee) = source::dotted_exact(&t[1..open]) else {
        return;
    };
    let Some((object, method)) = callee.rsplit_once('.') else {
        return;
    };
    let methods: Vec<String> = match method {
        m if VERBS.contains(&m) => vec![m.to_uppercase()],
        "websocket" => vec!["WS".into()],
        "api_route" => Vec::new(),
        _ => return,
    };
    let Some(key) = project.resolve_variable(path, object) else {
        return;
    };
    if !objects.contains_key(&key) {
        return;
    }
    let Some((args, _)) = source::call_args(t, open) else {
        return;
    };
    let Some(path_arg) = source::positional(&args, 0).or_else(|| source::keyword(&args, "path"))
    else {
        return;
    };
    let Some(rel) = path_arg.string() else {
        return;
    };
    let methods = if method == "api_route" {
        let m = source::keyword(&args, "methods")
            .map(|a| a.strings())
            .unwrap_or_default();
        if m.is_empty() {
            vec!["GET".into()]
        } else {
            m.iter().map(|x| x.to_uppercase()).collect()
        }
    } else {
        methods
    };
    let name = source::keyword(&args, "name")
        .and_then(|a| a.string())
        .unwrap_or_else(|| func.text.clone());
    let module = project.module_of(path);
    for prefix in mounts(objects, &key, 0, &mut BTreeSet::new()) {
        let full = format!("{prefix}{rel}");
        let full = if full.is_empty() {
            "/".to_string()
        } else {
            full
        };
        out.push(Route {
            framework: "fastapi".into(),
            methods: methods.clone(),
            sample_path: sample_path(&full),
            path: full,
            name: Some(name.clone()),
            handler: format!("{module}.{}", func.text),
            location: project.locate(path, path_arg.tokens[0].start),
            handler_location: Some(project.locate(path, func.start)),
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::project;

    #[test]
    fn routers_with_prefixes() {
        let p = project(&[
            ("pyproject.toml", "[project]\nname='x'\ndependencies=['fastapi']\n"),
            ("src/api/__init__.py", ""),
            ("src/api/main.py", "from fastapi import FastAPI\nfrom api.routers import orders\nfrom .routers.users import router as users_router\n\napp = FastAPI()\napp.include_router(orders.router, prefix='/v1')\napp.include_router(users_router)\n\n@app.get('/health')\ndef health():\n    pass\n\n@app.websocket('/ws')\nasync def ws(socket):\n    pass\n"),
            ("src/api/routers/__init__.py", ""),
            ("src/api/routers/orders.py", "from fastapi import APIRouter\n\nrouter = APIRouter(prefix='/orders', tags=['orders'])\nitems = APIRouter(prefix='/{order_id}/items')\nrouter.include_router(items)\n\n@router.get('/{order_id}')\nasync def read_order(order_id: int):\n    pass\n\n@router.api_route('', methods=['POST', 'put'])\ndef save():\n    pass\n\n@items.delete('/{item_id}', name='remove-item')\ndef remove(order_id: int, item_id: int):\n    pass\n"),
            ("src/api/routers/users.py", "from fastapi import APIRouter\nrouter = APIRouter(prefix='/users')\n\n@router.post('/')\ndef create():\n    pass\n"),
        ]);
        let r: Vec<(String, String, String)> = p
            .info
            .routes
            .iter()
            .map(|r| {
                (
                    r.methods.join(","),
                    r.path.clone(),
                    r.name.clone().unwrap_or_default(),
                )
            })
            .collect();
        assert_eq!(
            r,
            [
                ("GET".into(), "/health".into(), "health".into()),
                ("WS".into(), "/ws".into(), "ws".into()),
                (
                    "GET".into(),
                    "/v1/orders/{order_id}".into(),
                    "read_order".into()
                ),
                ("POST,PUT".into(), "/v1/orders".into(), "save".into()),
                (
                    "DELETE".into(),
                    "/v1/orders/{order_id}/items/{item_id}".into(),
                    "remove-item".into()
                ),
                ("POST".into(), "/users/".into(), "create".into()),
            ]
        );
        assert_eq!(p.info.routes[4].sample_path, "/v1/orders/1/items/1");
        assert_eq!(p.info.routes[2].handler, "api.routers.orders.read_order");
    }
}
