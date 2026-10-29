//! The debugger console (PHPStorm's "Console" tab of a debug session): values printed by
//! logpoints and evaluations, and completion of variables and members while typing a PHP
//! expression.

use serde::{Deserialize, Serialize};

use crate::protocol::Property;

/// A value as one console line: scalars as PHP prints them in the debugger (strings
/// quoted), compound values with their class or size.
pub fn display_value(p: &Property) -> String {
    match p.r#type.as_str() {
        "string" => format!("\"{}\"", p.value.as_deref().unwrap_or_default()),
        "null" | "uninitialized" => "null".into(),
        "bool" => match p.value.as_deref() {
            Some("1") | Some("true") => "true".into(),
            _ => "false".into(),
        },
        "array" => format!("array({})", p.num_children),
        "object" => match &p.classname {
            Some(c) => format!("{{{c}}}"),
            None => "{object}".into(),
        },
        _ => p.value.clone().unwrap_or_default(),
    }
}

/// What the expression before the caret completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionTarget {
    /// `$ord|`: variables of the frame starting with `prefix` (without `$`).
    Variable { prefix: String },
    /// `$order->cus|` or `$order->items[0]->na|`: members of `object`.
    Member { object: String, prefix: String },
    /// `Order::ST|`: nothing the engine can list cheaply.
    None,
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The completion target of `text` with the caret at its end.
pub fn completion_target(text: &str) -> CompletionTarget {
    let chars: Vec<char> = text.chars().collect();
    let mut i = chars.len();
    while i > 0 && is_ident(chars[i - 1]) {
        i -= 1;
    }
    let prefix: String = chars[i..].iter().collect();
    if i > 0 && chars[i - 1] == '$' {
        return CompletionTarget::Variable { prefix };
    }
    if i >= 2 && chars[i - 1] == '>' && chars[i - 2] == '-' {
        let end = i - 2;
        if let Some(start) = object_start(&chars[..end]) {
            let object: String = chars[start..end].iter().collect();
            return CompletionTarget::Member { object, prefix };
        }
    }
    CompletionTarget::None
}

/// Start of a variable access chain ending at `chars.len()`: `$a`, `$a->b`, `$a['x'][0]`.
fn object_start(chars: &[char]) -> Option<usize> {
    let mut i = chars.len();
    loop {
        if i == 0 {
            return None;
        }
        match chars[i - 1] {
            ']' => {
                // Skip a balanced `[...]` index.
                let mut depth = 0;
                loop {
                    if i == 0 {
                        return None;
                    }
                    i -= 1;
                    match chars[i] {
                        ']' => depth += 1,
                        '[' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            }
            c if is_ident(c) => {
                while i > 0 && is_ident(chars[i - 1]) {
                    i -= 1;
                }
                if i > 0 && chars[i - 1] == '$' {
                    return Some(i - 1);
                }
                if i >= 2 && chars[i - 1] == '>' && chars[i - 2] == '-' {
                    i -= 2;
                } else {
                    return None;
                }
            }
            _ => return None,
        }
    }
}

/// One completion item of the console or the Evaluate dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    /// `$order` or `customer`.
    pub label: String,
    /// The text replacing the typed prefix.
    pub insert: String,
    /// DBGp type or class shown on the right.
    pub detail: String,
}

fn detail(p: &Property) -> String {
    p.classname.clone().unwrap_or_else(|| p.r#type.clone())
}

/// Variables starting with `prefix` (case-insensitive), `$this` and locals first.
pub fn complete_variables(props: &[Property], prefix: &str) -> Vec<Completion> {
    let lower = prefix.to_lowercase();
    let mut out: Vec<Completion> = props
        .iter()
        .filter_map(|p| {
            let name = p.name.trim_start_matches('$');
            name.to_lowercase().starts_with(&lower).then(|| Completion {
                label: format!("${name}"),
                insert: name.to_string(),
                detail: detail(p),
            })
        })
        .collect();
    out.sort_by_key(|c| {
        (
            c.label != "$this",
            c.label.starts_with("$_"),
            c.label.clone(),
        )
    });
    out.dedup_by(|a, b| a.label == b.label);
    out
}

/// Properties of an object starting with `prefix`. Static and numeric members are left
/// out (`->` cannot reach them).
pub fn complete_members(object: &Property, prefix: &str) -> Vec<Completion> {
    let lower = prefix.to_lowercase();
    let mut out: Vec<Completion> = object
        .children
        .iter()
        .filter(|c| !c.facet.as_deref().unwrap_or("").contains("static"))
        .filter(|c| !c.name.starts_with(|ch: char| ch.is_ascii_digit()) && !c.name.contains(':'))
        .filter(|c| c.name.to_lowercase().starts_with(&lower))
        .map(|c| Completion {
            label: c.name.clone(),
            insert: c.name.clone(),
            detail: detail(c),
        })
        .collect();
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out.dedup_by(|a, b| a.label == b.label);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prop(name: &str, ty: &str) -> Property {
        Property {
            name: name.into(),
            fullname: name.into(),
            r#type: ty.into(),
            ..Default::default()
        }
    }

    #[test]
    fn values_print_like_the_debugger() {
        let mut s = prop("$a", "string");
        s.value = Some("PAID".into());
        assert_eq!(display_value(&s), "\"PAID\"");
        let mut o = prop("$o", "object");
        o.classname = Some("App\\Models\\Order".into());
        assert_eq!(display_value(&o), "{App\\Models\\Order}");
        let mut a = prop("$l", "array");
        a.num_children = 3;
        assert_eq!(display_value(&a), "array(3)");
        let mut b = prop("$b", "bool");
        b.value = Some("1".into());
        assert_eq!(display_value(&b), "true");
        assert_eq!(display_value(&prop("$n", "null")), "null");
    }

    #[test]
    fn finds_what_to_complete() {
        use CompletionTarget::*;
        assert_eq!(
            completion_target("count($ord"),
            Variable {
                prefix: "ord".into()
            }
        );
        assert_eq!(completion_target("$"), Variable { prefix: "".into() });
        assert_eq!(
            completion_target("$order->cus"),
            Member {
                object: "$order".into(),
                prefix: "cus".into()
            }
        );
        assert_eq!(
            completion_target("strlen($order->items[0]['x']->na"),
            Member {
                object: "$order->items[0]['x']".into(),
                prefix: "na".into()
            }
        );
        assert_eq!(
            completion_target("$this->repo->"),
            Member {
                object: "$this->repo".into(),
                prefix: "".into()
            }
        );
        assert_eq!(completion_target("Order::ST"), None);
        assert_eq!(completion_target("foo()->x"), None);
        assert_eq!(completion_target("1 + 2"), None);
    }

    #[test]
    fn completes_variables_and_members() {
        let vars = vec![
            prop("$order", "object"),
            prop("$_SERVER", "array"),
            prop("$this", "object"),
            prop("$total", "float"),
            prop("$orders", "array"),
        ];
        let all: Vec<_> = complete_variables(&vars, "")
            .into_iter()
            .map(|c| c.label)
            .collect();
        assert_eq!(all, ["$this", "$order", "$orders", "$total", "$_SERVER"]);
        let ord = complete_variables(&vars, "OR");
        assert_eq!(ord.len(), 2);
        assert_eq!(ord[0].insert, "order");

        let mut obj = prop("$order", "object");
        let mut stat = prop("count", "int");
        stat.facet = Some("public static".into());
        obj.children = vec![prop("status", "string"), prop("customer", "object"), stat];
        let members = complete_members(&obj, "");
        assert_eq!(
            members.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(),
            ["customer", "status"]
        );
        assert_eq!(complete_members(&obj, "st")[0].insert, "status");
    }
}
