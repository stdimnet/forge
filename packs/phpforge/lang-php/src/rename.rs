//! PHP parts of Rename (Shift+F6) on the index: the dialog label for a symbol and the name
//! clashes PHPStorm reports as conflicts (a class or member with the new name already
//! exists). The edit itself is built from Find Usages (`navigate::find_usages`) with
//! `forge_codeintel::edit_from_locations`.

use forge_codeintel::RenameConflict;

use crate::index::SymbolHit;
use crate::model::{short_name, SymbolKind};
use crate::navigate::SymbolRef;

fn kind_word(symbol: &SymbolRef) -> &'static str {
    match symbol.kind_name() {
        "enumCase" => "enum case",
        other => other,
    }
}

/// PHPStorm's dialog label: "Rename class 'User' and its usages to:".
pub fn rename_label(symbol: &SymbolRef) -> String {
    let name = match symbol {
        SymbolRef::Class { fqn } | SymbolRef::Constant { fqn } => short_name(fqn).to_string(),
        SymbolRef::Function { fqn } => format!("{}()", short_name(fqn)),
        SymbolRef::Variable { name, .. } => name.clone(),
        SymbolRef::Member { member, name, .. } => match member {
            SymbolKind::Method => format!("{name}()"),
            SymbolKind::Property => format!("${name}"),
            _ => name.clone(),
        },
    };
    format!("Rename {} '{name}' and its usages to:", kind_word(symbol))
}

fn namespace_of(fqn: &str) -> &str {
    fqn.rsplit_once('\\').map_or("", |(ns, _)| ns)
}

fn qualified(ns: &str, name: &str) -> String {
    if ns.is_empty() {
        name.to_string()
    } else {
        format!("{ns}\\{name}")
    }
}

fn same_member_kind(a: SymbolKind, b: SymbolKind) -> bool {
    a == b
        || matches!(
            (a, b),
            (SymbolKind::Constant, SymbolKind::EnumCase)
                | (SymbolKind::EnumCase, SymbolKind::Constant)
        )
}

/// Symbols that already use `new_name` where `symbol` lives, from a symbol search for the
/// new name.
pub fn clashes(symbol: &SymbolRef, new_name: &str, hits: &[SymbolHit]) -> Vec<RenameConflict> {
    let bare = new_name.trim_start_matches('$');
    let conflict =
        |what: String, h: &SymbolHit| RenameConflict::at(what, h.path.clone(), Some(h.line));
    let mut out = Vec::new();
    for h in hits {
        match symbol {
            SymbolRef::Class { fqn } => {
                let target = qualified(namespace_of(fqn), bare);
                if h.kind.is_class_like()
                    && h.fqn.eq_ignore_ascii_case(&target)
                    && !h.fqn.eq_ignore_ascii_case(fqn)
                {
                    out.push(conflict(format!("Class {target} already exists"), h));
                }
            }
            SymbolRef::Function { fqn } => {
                let target = qualified(namespace_of(fqn), bare);
                if h.kind == SymbolKind::Function && h.fqn.eq_ignore_ascii_case(&target) {
                    out.push(conflict(format!("Function {target}() already exists"), h));
                }
            }
            SymbolRef::Constant { fqn } => {
                let target = qualified(namespace_of(fqn), bare);
                if h.kind == SymbolKind::Constant && h.fqn == target {
                    out.push(conflict(format!("Constant {target} already exists"), h));
                }
            }
            SymbolRef::Member {
                class,
                member,
                name,
            } => {
                let same_name = if *member == SymbolKind::Method {
                    h.name.eq_ignore_ascii_case(bare) && !h.name.eq_ignore_ascii_case(name)
                } else {
                    h.name.trim_start_matches('$') == bare && bare != name
                };
                if same_member_kind(h.kind, *member)
                    && same_name
                    && h.container.eq_ignore_ascii_case(class)
                {
                    let shown = match member {
                        SymbolKind::Method => format!("Method {bare}()"),
                        SymbolKind::Property => format!("Property ${bare}"),
                        _ => format!("Constant {bare}"),
                    };
                    out.push(conflict(
                        format!("{shown} is already defined in class {}", short_name(class)),
                        h,
                    ));
                }
            }
            SymbolRef::Variable { .. } => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(name: &str, fqn: &str, kind: SymbolKind, container: &str) -> SymbolHit {
        SymbolHit {
            name: name.into(),
            fqn: fqn.into(),
            kind,
            container: container.into(),
            path: "app/Models/Customer.php".into(),
            offset: 0,
            line: 7,
            column: 1,
            score: 0,
            positions: vec![],
            vendor: false,
            deprecated: false,
        }
    }

    #[test]
    fn labels_like_phpstorm() {
        let class = SymbolRef::Class {
            fqn: "App\\Models\\User".into(),
        };
        assert_eq!(
            rename_label(&class),
            "Rename class 'User' and its usages to:"
        );
        let method = SymbolRef::Member {
            class: "App\\Models\\User".into(),
            member: SymbolKind::Method,
            name: "orders".into(),
        };
        assert_eq!(
            rename_label(&method),
            "Rename method 'orders()' and its usages to:"
        );
        let prop = SymbolRef::Member {
            class: "App\\Models\\User".into(),
            member: SymbolKind::Property,
            name: "email".into(),
        };
        assert_eq!(
            rename_label(&prop),
            "Rename property '$email' and its usages to:"
        );
    }

    #[test]
    fn reports_existing_classes_and_members() {
        let class = SymbolRef::Class {
            fqn: "App\\Models\\User".into(),
        };
        let hits = vec![
            hit(
                "Customer",
                "App\\Models\\Customer",
                SymbolKind::Class,
                "App\\Models",
            ),
            hit(
                "Customer",
                "App\\Billing\\Customer",
                SymbolKind::Class,
                "App\\Billing",
            ),
        ];
        let found = clashes(&class, "Customer", &hits);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].message,
            "Class App\\Models\\Customer already exists"
        );
        assert_eq!(found[0].path.as_deref(), Some("app/Models/Customer.php"));
        assert_eq!(found[0].line, Some(7));

        let method = SymbolRef::Member {
            class: "App\\Models\\User".into(),
            member: SymbolKind::Method,
            name: "orders".into(),
        };
        let hits = vec![
            hit(
                "purchases",
                "App\\Models\\User::purchases",
                SymbolKind::Method,
                "App\\Models\\User",
            ),
            hit(
                "purchases",
                "App\\Models\\Order::purchases",
                SymbolKind::Method,
                "App\\Models\\Order",
            ),
            hit(
                "purchases",
                "App\\Models\\User::$purchases",
                SymbolKind::Property,
                "App\\Models\\User",
            ),
        ];
        let found = clashes(&method, "purchases", &hits);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].message,
            "Method purchases() is already defined in class User"
        );
        // Renaming to the same name (or its case) is not a clash with itself.
        let hits = vec![hit("orders", "", SymbolKind::Method, "App\\Models\\User")];
        assert!(clashes(&method, "Orders", &hits).is_empty());

        let prop = SymbolRef::Member {
            class: "App\\Models\\User".into(),
            member: SymbolKind::Property,
            name: "email".into(),
        };
        let hits = vec![hit("name", "", SymbolKind::Property, "App\\Models\\User")];
        assert_eq!(
            clashes(&prop, "$name", &hits)[0].message,
            "Property $name is already defined in class User"
        );
    }
}
