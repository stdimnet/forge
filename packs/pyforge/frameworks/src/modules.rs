//! Python modules of a project: which file a dotted module name is (`shop.views` →
//! `shop/views.py`), a file's module name, and the names a file imports, so a reference
//! such as `views.index` in `urls.py` finds `def index` in `shop/views.py`.

use std::collections::BTreeMap;

use crate::source::{Kind, Line};

/// Folders that hold top-level packages: the project root, `src/` and the folders of
/// `manage.py` files (Django projects in a subfolder).
pub fn source_roots(paths: &[&str]) -> Vec<String> {
    let mut roots = vec![String::new()];
    if paths.iter().any(|p| p.starts_with("src/")) {
        roots.push("src".into());
    }
    for p in paths {
        if let Some(dir) = p.strip_suffix("/manage.py") {
            if !roots.iter().any(|r| r == dir) {
                roots.push(dir.to_string());
            }
        }
    }
    roots
}

pub fn join(dir: &str, rel: &str) -> String {
    let rel = rel.trim_start_matches("./");
    if dir.is_empty() {
        rel.to_string()
    } else if rel.is_empty() {
        dir.to_string()
    } else {
        format!("{dir}/{rel}")
    }
}

pub fn parent(path: &str) -> &str {
    path.rfind('/').map(|i| &path[..i]).unwrap_or("")
}

/// The module of a file below one of `roots` (the longest root that contains it).
pub fn module_of(path: &str, roots: &[String]) -> String {
    let mut best: Option<&str> = None;
    for r in roots {
        let inside = r.is_empty() || path.starts_with(&format!("{r}/"));
        if inside && best.is_none_or(|b| r.len() > b.len()) {
            best = Some(r);
        }
    }
    let rel = match best {
        Some(r) if !r.is_empty() => &path[r.len() + 1..],
        _ => path,
    };
    let rel = rel.strip_suffix(".py").unwrap_or(rel);
    let rel = rel.strip_suffix("/__init__").unwrap_or(rel);
    rel.replace('/', ".")
}

/// The file of a module: `<root>/a/b.py` or `<root>/a/b/__init__.py`.
pub fn file_of<'a>(
    module: &str,
    roots: &[String],
    exists: impl Fn(&str) -> bool + 'a,
) -> Option<String> {
    let rel = module.replace('.', "/");
    for r in roots {
        for candidate in [format!("{rel}.py"), format!("{rel}/__init__.py")] {
            let p = join(r, &candidate);
            if exists(&p) {
                return Some(p);
            }
        }
    }
    None
}

/// What a name imported into a file refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub module: String,
    /// `from module import attr`: the attribute (or submodule).
    pub attr: Option<String>,
}

/// Absolute module of `from <dots><rest> import` in the file whose module is `current`.
fn absolute(current: &str, is_package: bool, dots: usize, rest: &str) -> String {
    if dots == 0 {
        return rest.to_string();
    }
    let mut parts: Vec<&str> = current.split('.').filter(|p| !p.is_empty()).collect();
    // `from . import x` in a module is relative to its package.
    let up = if is_package { dots - 1 } else { dots };
    for _ in 0..up {
        parts.pop();
    }
    if !rest.is_empty() {
        parts.push(rest);
    }
    parts.join(".")
}

/// The names a file binds with `import` statements (module level and nested alike).
pub fn imports(lines: &[Line], module: &str, is_package: bool) -> BTreeMap<String, Imported> {
    let mut out = BTreeMap::new();
    for line in lines {
        let t = &line.tokens;
        if t.first().is_some_and(|x| x.is_name("import")) {
            // import a.b as c, d
            let mut i = 1;
            while i < t.len() {
                let start = i;
                while i < t.len()
                    && (t[i].kind == Kind::Name && !t[i].is_name("as") || t[i].is_op("."))
                {
                    i += 1;
                }
                let dotted: String = t[start..i].iter().map(|x| x.text.as_str()).collect();
                if dotted.is_empty() {
                    break;
                }
                if t.get(i).is_some_and(|x| x.is_name("as")) {
                    if let Some(alias) = t.get(i + 1) {
                        out.insert(
                            alias.text.clone(),
                            Imported {
                                module: dotted,
                                attr: None,
                            },
                        );
                    }
                    i += 2;
                } else {
                    let top = dotted.split('.').next().unwrap_or("").to_string();
                    out.insert(
                        top.clone(),
                        Imported {
                            module: top,
                            attr: None,
                        },
                    );
                }
                if t.get(i).is_some_and(|x| x.is_op(",")) {
                    i += 1;
                } else {
                    break;
                }
            }
        } else if t.first().is_some_and(|x| x.is_name("from")) {
            let Some(imp) = t.iter().position(|x| x.is_name("import")) else {
                continue;
            };
            let mut dots = 0;
            let mut rest = String::new();
            for x in &t[1..imp] {
                if x.is_op(".") && rest.is_empty() {
                    dots += 1;
                } else {
                    rest.push_str(&x.text);
                }
            }
            let base = absolute(module, is_package, dots, &rest);
            let mut i = imp + 1;
            while i < t.len() {
                if t[i].is_op("(") || t[i].is_op(")") || t[i].is_op(",") {
                    i += 1;
                    continue;
                }
                if t[i].kind != Kind::Name {
                    break;
                }
                let name = t[i].text.clone();
                let mut bound = name.clone();
                if t.get(i + 1).is_some_and(|x| x.is_name("as")) {
                    if let Some(a) = t.get(i + 2) {
                        bound = a.text.clone();
                    }
                    i += 3;
                } else {
                    i += 1;
                }
                if name != "*" {
                    out.insert(
                        bound,
                        Imported {
                            module: base.clone(),
                            attr: Some(name),
                        },
                    );
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::lines;

    #[test]
    fn modules_and_files() {
        let roots = source_roots(&["src/app/main.py", "backend/manage.py"]);
        assert_eq!(roots, ["", "src", "backend"]);
        assert_eq!(module_of("src/app/main.py", &roots), "app.main");
        assert_eq!(module_of("backend/shop/__init__.py", &roots), "shop");
        assert_eq!(module_of("tools/x.py", &roots), "tools.x");
        let files = ["backend/shop/views.py", "backend/shop/__init__.py"];
        assert_eq!(
            file_of("shop.views", &roots, |p| files.contains(&p)).as_deref(),
            Some("backend/shop/views.py")
        );
        assert_eq!(
            file_of("shop", &roots, |p| files.contains(&p)).as_deref(),
            Some("backend/shop/__init__.py")
        );
    }

    #[test]
    fn import_statements() {
        let src = "import os\nimport shop.views as v\nfrom . import views\nfrom .models import (Order, Item as I)\nfrom ..core import util\nfrom django.urls import path, include\n";
        let map = imports(&lines(src), "shop.urls", false);
        assert_eq!(
            map["os"],
            Imported {
                module: "os".into(),
                attr: None
            }
        );
        assert_eq!(map["v"].module, "shop.views");
        assert_eq!(
            map["views"],
            Imported {
                module: "shop".into(),
                attr: Some("views".into())
            }
        );
        assert_eq!(
            map["I"],
            Imported {
                module: "shop.models".into(),
                attr: Some("Item".into())
            }
        );
        assert_eq!(map["Order"].module, "shop.models");
        assert_eq!(map["util"].module, "core");
        assert_eq!(map["include"].module, "django.urls");
        let pkg = imports(&lines("from . import views\n"), "shop", true);
        assert_eq!(pkg["views"].module, "shop");
    }
}
