//! Name resolution through namespaces and use-imports (PHP's rules, simplified to what an
//! index needs: classes and type hints).

use crate::model::{short_name, UseImport, UseKind};

/// Built-in type keywords that are never namespaced.
const BUILTIN_TYPES: &[&str] = &[
    "self",
    "static",
    "parent",
    "int",
    "float",
    "string",
    "bool",
    "array",
    "callable",
    "iterable",
    "object",
    "mixed",
    "void",
    "null",
    "never",
    "false",
    "true",
    "resource",
    "integer",
    "boolean",
    "double",
    "$this",
    "class-string",
    "list",
    "non-empty-array",
    "non-empty-list",
    "non-empty-string",
    "positive-int",
    "negative-int",
    "array-key",
    "scalar",
    "numeric",
];

pub fn is_builtin_type(name: &str) -> bool {
    BUILTIN_TYPES.iter().any(|b| b.eq_ignore_ascii_case(name))
}

/// Resolve a class name as written to its fully qualified name (no leading backslash).
pub fn resolve_class_name(name: &str, namespace: &str, uses: &[UseImport]) -> String {
    let name = name.trim();
    if let Some(stripped) = name.strip_prefix('\\') {
        return stripped.to_string();
    }
    if is_builtin_type(name) {
        return name.to_string();
    }
    if let Some(rest) = name.strip_prefix("namespace\\") {
        return join(namespace, rest);
    }
    let (first, rest) = match name.split_once('\\') {
        Some((f, r)) => (f, Some(r)),
        None => (name, None),
    };
    for u in uses.iter().filter(|u| u.kind == UseKind::Class) {
        if u.local_name().eq_ignore_ascii_case(first) {
            return match rest {
                Some(r) => format!("{}\\{r}", u.fqn),
                None => u.fqn.clone(),
            };
        }
    }
    join(namespace, name)
}

fn join(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_string()
    } else {
        format!("{namespace}\\{name}")
    }
}

/// Apply `f` to every class-name atom of a type expression (`?Foo`, `A|B`, `A&B`,
/// `Collection<int, Order>`, `Foo[]`), leaving punctuation, builtins, quoted strings and
/// array-shape keys untouched.
pub fn map_type_names(ty: &str, mut f: impl FnMut(&str) -> String) -> String {
    let chars: Vec<char> = ty.chars().collect();
    let mut out = String::with_capacity(ty.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' || c == '"' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != c {
                i += 1;
            }
            i = (i + 1).min(chars.len());
            out.extend(&chars[start..i]);
            continue;
        }
        if !is_word_char(c) {
            out.push(c);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && is_word_char(chars[i]) {
            i += 1;
        }
        let word: String = chars[start..i].iter().collect();
        // Array-shape keys (`array{key: int}`) are followed by a single colon.
        let mut j = i;
        while j < chars.len() && chars[j] == ' ' {
            j += 1;
        }
        let is_key = j < chars.len()
            && chars[j] == ':'
            && chars.get(j + 1) != Some(&':')
            && chars[..start]
                .iter()
                .rev()
                .find(|c| **c != ' ')
                .is_some_and(|c| *c == '{' || *c == ',');
        let first = word.chars().next().unwrap_or(' ');
        if is_key
            || first == '$'
            || first.is_ascii_digit()
            || first == '-'
            || is_builtin_type(&word)
        {
            out.push_str(&word);
        } else {
            out.push_str(&f(&word));
        }
    }
    out
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\\' || c == '$' || c == '-'
}

/// Resolve every class name inside a type expression to its FQN (with leading `\`).
pub fn resolve_type(ty: &str, namespace: &str, uses: &[UseImport]) -> String {
    map_type_names(ty, |n| {
        format!("\\{}", resolve_class_name(n, namespace, uses))
    })
}

/// Shorten a fully-resolved type (`\App\Models\User`) for use in a file with the given
/// namespace and imports: imported or same-namespace names become short names, everything
/// else stays fully qualified.
pub fn shorten_type(ty: &str, namespace: &str, uses: &[UseImport]) -> String {
    map_type_names(ty, |n| {
        let fqn = n.trim_start_matches('\\');
        if !n.starts_with('\\') {
            return n.to_string();
        }
        if let Some(u) = uses
            .iter()
            .find(|u| u.kind == UseKind::Class && u.fqn.eq_ignore_ascii_case(fqn))
        {
            return u.local_name().to_string();
        }
        let short = short_name(fqn);
        let ns = fqn.rsplit_once('\\').map(|(ns, _)| ns).unwrap_or("");
        if ns.eq_ignore_ascii_case(namespace)
            && !uses
                .iter()
                .any(|u| u.local_name().eq_ignore_ascii_case(short))
        {
            return short.to_string();
        }
        format!("\\{fqn}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Range;

    fn imp(fqn: &str, alias: Option<&str>) -> UseImport {
        UseImport {
            kind: UseKind::Class,
            fqn: fqn.into(),
            alias: alias.map(Into::into),
            range: Range::default(),
        }
    }

    #[test]
    fn resolves_through_imports_aliases_and_namespace() {
        let uses = vec![
            imp(
                "Illuminate\\Foundation\\Auth\\User",
                Some("Authenticatable"),
            ),
            imp("Doctrine\\ORM\\Mapping", Some("ORM")),
            imp("App\\Models\\Order", None),
        ];
        let ns = "App\\Http";
        assert_eq!(
            resolve_class_name("Authenticatable", ns, &uses),
            "Illuminate\\Foundation\\Auth\\User"
        );
        assert_eq!(
            resolve_class_name("ORM\\Column", ns, &uses),
            "Doctrine\\ORM\\Mapping\\Column"
        );
        assert_eq!(resolve_class_name("order", ns, &uses), "App\\Models\\Order");
        assert_eq!(resolve_class_name("Kernel", ns, &uses), "App\\Http\\Kernel");
        assert_eq!(resolve_class_name("\\DateTime", ns, &uses), "DateTime");
        assert_eq!(resolve_class_name("self", ns, &uses), "self");
        assert_eq!(resolve_class_name("Foo", "", &uses), "Foo");
        assert_eq!(
            resolve_class_name("namespace\\Foo", ns, &uses),
            "App\\Http\\Foo"
        );
    }

    #[test]
    fn resolves_and_shortens_compound_types() {
        let uses = vec![imp("App\\Models\\Order", None)];
        let resolved = resolve_type("?Order|Collection<int, Order>|null", "App\\Services", &uses);
        assert_eq!(
            resolved,
            "?\\App\\Models\\Order|\\App\\Services\\Collection<int, \\App\\Models\\Order>|null"
        );
        assert_eq!(
            shorten_type(&resolved, "App\\Services", &uses),
            "?Order|Collection<int, Order>|null"
        );
        assert_eq!(shorten_type("\\Foo\\Bar", "App", &[]), "\\Foo\\Bar");
        assert_eq!(
            resolve_type("array{a: Foo, 'b': string}", "App", &[]),
            "array{a: \\App\\Foo, 'b': string}"
        );
    }
}
