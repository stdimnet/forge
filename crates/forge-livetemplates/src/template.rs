//! Template text: plain text and `$NAME$` variables. `$$` is a literal dollar sign;
//! `$END$` (final caret position) and `$SELECTION$` (the surrounded text) are predefined.

pub const END: &str = "END";
pub const SELECTION: &str = "SELECTION";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Text(String),
    Var(String),
}

fn is_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Split template text into text and variables.
pub fn parse(text: &str) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut buf = String::new();
    let mut rest = text;
    while let Some(pos) = rest.find('$') {
        buf.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        if let Some(stripped) = after.strip_prefix('$') {
            buf.push('$');
            rest = stripped;
            continue;
        }
        match after.find('$') {
            Some(end) if is_name(&after[..end]) => {
                if !buf.is_empty() {
                    out.push(Segment::Text(std::mem::take(&mut buf)));
                }
                out.push(Segment::Var(after[..end].to_string()));
                rest = &after[end + 1..];
            }
            _ => {
                buf.push('$');
                rest = after;
            }
        }
    }
    buf.push_str(rest);
    if !buf.is_empty() {
        out.push(Segment::Text(buf));
    }
    out
}

/// User variables of a template text in order of appearance (without `END`/`SELECTION`),
/// as PHPStorm's "Edit Template Variables" dialog lists them.
pub fn variable_names(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for seg in parse(text) {
        if let Segment::Var(name) = seg {
            if name != END && name != SELECTION && !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// Whether the template wraps a selection (offered by Surround With).
pub fn uses_selection(text: &str) -> bool {
    parse(text)
        .iter()
        .any(|s| matches!(s, Segment::Var(n) if n == SELECTION))
}

#[cfg(test)]
mod tests {
    use super::*;
    use Segment::*;

    #[test]
    fn parses_variables_and_dollars() {
        assert_eq!(
            parse("foreach ($ARRAY$ as $$$VAR$) {\n$END$\n}"),
            vec![
                Text("foreach (".into()),
                Var("ARRAY".into()),
                Text(" as $".into()),
                Var("VAR".into()),
                Text(") {\n".into()),
                Var("END".into()),
                Text("\n}".into()),
            ]
        );
        assert_eq!(parse("$a = 1; $ b$"), vec![Text("$a = 1; $ b$".into())]);
        assert_eq!(parse("$$this->x"), vec![Text("$this->x".into())]);
    }

    #[test]
    fn lists_user_variables() {
        assert_eq!(variable_names("$A$ $B$ $A$ $END$ $SELECTION$"), ["A", "B"]);
        assert!(uses_selection("if ($C$) { $SELECTION$ }"));
        assert!(!uses_selection("$$SELECTION"));
    }
}
