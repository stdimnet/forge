//! The parts of ActiveSupport's inflector Rails' conventions need: plural and singular
//! nouns (`article` ↔ `articles`, `person` ↔ `people`), `camelize` and `underscore`.

const IRREGULAR: &[(&str, &str)] = &[
    ("person", "people"),
    ("man", "men"),
    ("woman", "women"),
    ("child", "children"),
    ("sex", "sexes"),
    ("move", "moves"),
    ("zombie", "zombies"),
    ("ox", "oxen"),
    ("foot", "feet"),
    ("tooth", "teeth"),
    ("goose", "geese"),
    ("mouse", "mice"),
    ("criterion", "criteria"),
    ("datum", "data"),
];

const UNCOUNTABLE: &[&str] = &[
    "equipment",
    "information",
    "rice",
    "money",
    "species",
    "series",
    "fish",
    "sheep",
    "jeans",
    "police",
    "news",
    "metadata",
    "feedback",
];

fn split_last(word: &str) -> (&str, &str) {
    match word.rfind('_') {
        Some(i) => (&word[..=i], &word[i + 1..]),
        None => ("", word),
    }
}

fn keep_case(original: &str, replacement: &str) -> String {
    if original.chars().next().is_some_and(char::is_uppercase) {
        let mut c = replacement.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => String::new(),
        }
    } else {
        replacement.to_string()
    }
}

/// `article` → `articles`, `category` → `categories`, `person` → `people`,
/// `line_item` → `line_items`.
pub fn pluralize(word: &str) -> String {
    let (head, last) = split_last(word);
    let lower = last.to_ascii_lowercase();
    if lower.is_empty() || UNCOUNTABLE.contains(&lower.as_str()) {
        return word.to_string();
    }
    if let Some((_, p)) = IRREGULAR.iter().find(|(s, p)| *s == lower || *p == lower) {
        return format!("{head}{}", keep_case(last, p));
    }
    let l = last;
    let plural = if lower.ends_with("quiz") {
        format!("{l}zes")
    } else if lower.ends_with("matrix") || lower.ends_with("vertex") || lower.ends_with("index") {
        format!("{}ices", &l[..l.len() - 2])
    } else if lower.ends_with("ss")
        || lower.ends_with("sh")
        || lower.ends_with("ch")
        || lower.ends_with('x')
        || lower.ends_with('z')
    {
        format!("{l}es")
    } else if lower.ends_with("us") && lower.len() >= 3 {
        // status → statuses, bus → buses
        format!("{l}es")
    } else if lower.ends_with("sis") {
        format!("{}ses", &l[..l.len() - 3])
    } else if lower.ends_with('y')
        && !matches!(
            lower.chars().rev().nth(1),
            Some('a' | 'e' | 'i' | 'o' | 'u')
        )
    {
        format!("{}ies", &l[..l.len() - 1])
    } else if lower.ends_with("fe") {
        format!("{}ves", &l[..l.len() - 2])
    } else if (lower.ends_with("lf") || lower.ends_with("rf") || lower.ends_with("af"))
        && !lower.ends_with("ff")
    {
        format!("{}ves", &l[..l.len() - 1])
    } else if lower.ends_with('s') {
        l.to_string()
    } else if lower.ends_with("tomato") || lower.ends_with("potato") || lower.ends_with("hero") {
        format!("{l}es")
    } else {
        format!("{l}s")
    };
    format!("{head}{plural}")
}

/// `articles` → `article`, `categories` → `category`, `people` → `person`.
pub fn singularize(word: &str) -> String {
    let (head, last) = split_last(word);
    let lower = last.to_ascii_lowercase();
    if lower.is_empty() || UNCOUNTABLE.contains(&lower.as_str()) {
        return word.to_string();
    }
    if let Some((s, _)) = IRREGULAR.iter().find(|(s, p)| *p == lower || *s == lower) {
        return format!("{head}{}", keep_case(last, s));
    }
    let l = last;
    let single = if lower.ends_with("quizzes") {
        l[..l.len() - 3].to_string()
    } else if lower.ends_with("matrices")
        || lower.ends_with("vertices")
        || lower.ends_with("indices")
    {
        format!("{}ex", &l[..l.len() - 4])
    } else if lower.ends_with("ies") && lower.len() > 3 && !lower.ends_with("movies") {
        format!("{}y", &l[..l.len() - 3])
    } else if lower.ends_with("ives") && lower.len() > 4 {
        format!("{}ife", &l[..l.len() - 4])
    } else if lower.ends_with("lves") || lower.ends_with("rves") || lower.ends_with("aves") {
        format!("{}f", &l[..l.len() - 3])
    } else if lower.ends_with("yses") || lower.ends_with("gnoses") {
        format!("{}is", &l[..l.len() - 2])
    } else if lower.ends_with("tuses")
        || lower.ends_with("buses")
        || lower.ends_with("ruses")
        || lower.ends_with("sses")
        || lower.ends_with("xes")
        || lower.ends_with("ches")
        || lower.ends_with("shes")
        || lower.ends_with("zzes")
        || lower.ends_with("oes")
    {
        l[..l.len() - 2].to_string()
    } else if lower.ends_with("ses") && lower.len() > 4 {
        // analyses → analysis, bases → basis is ambiguous: keep `base`.
        l[..l.len() - 1].to_string()
    } else if lower.ends_with("ss") || lower.ends_with("us") || lower.ends_with("is") {
        l.to_string()
    } else if let Some(s) = l.strip_suffix('s').or_else(|| l.strip_suffix('S')) {
        s.to_string()
    } else {
        l.to_string()
    };
    format!("{head}{single}")
}

/// `admin/line_items` → `Admin::LineItems`.
pub fn camelize(path: &str) -> String {
    path.split('/')
        .map(|seg| {
            seg.split('_')
                .map(|w| {
                    let mut c = w.chars();
                    match c.next() {
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                        None => String::new(),
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("::")
}

/// `Admin::LineItem` → `admin/line_item`, `HTMLParser` → `html_parser`.
pub fn underscore(name: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = name.replace("::", "/").chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev = i.checked_sub(1).map(|p| chars[p]);
            let next = chars.get(i + 1);
            if let Some(p) = prev {
                let boundary = p.is_lowercase()
                    || p.is_ascii_digit()
                    || (p.is_uppercase() && next.is_some_and(|n| n.is_lowercase()));
                if boundary && p != '/' {
                    out.push('_');
                }
            }
            out.extend(c.to_lowercase());
        } else if c == '-' {
            out.push('_');
        } else {
            out.push(c);
        }
    }
    out
}

/// The table of a model class: `LineItem` → `line_items`, `Person` → `people`.
pub fn tableize(class: &str) -> String {
    let last = class.rsplit("::").next().unwrap_or(class);
    pluralize(&underscore(last))
}

/// The model class of a table: `line_items` → `LineItem`.
pub fn classify(table: &str) -> String {
    camelize(&singularize(table))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plurals_and_singulars() {
        for (s, p) in [
            ("article", "articles"),
            ("category", "categories"),
            ("person", "people"),
            ("line_item", "line_items"),
            ("box", "boxes"),
            ("status", "statuses"),
            ("address", "addresses"),
            ("wife", "wives"),
            ("half", "halves"),
            ("day", "days"),
            ("analysis", "analyses"),
            ("news", "news"),
            ("bus", "buses"),
            ("hero", "heroes"),
            ("photo", "photos"),
            ("index", "indices"),
            ("user", "users"),
            ("comment", "comments"),
            ("profile", "profiles"),
            ("child", "children"),
        ] {
            assert_eq!(pluralize(s), p, "{s}");
            assert_eq!(singularize(p), s, "{p}");
        }
        assert_eq!(pluralize("articles"), "articles");
    }

    #[test]
    fn camel_and_snake_case() {
        assert_eq!(camelize("admin/line_items"), "Admin::LineItems");
        assert_eq!(underscore("Admin::LineItem"), "admin/line_item");
        assert_eq!(underscore("HTMLParser"), "html_parser");
        assert_eq!(underscore("Oauth2Token"), "oauth2_token");
        assert_eq!(tableize("Admin::Person"), "people");
        assert_eq!(classify("line_items"), "LineItem");
    }
}
