//! Code surrounders of Surround With (Ctrl+Alt+T) per language, in PHPStorm's order.
//! They are templates around `$SELECTION$`; statement surrounders wrap whole lines,
//! expression surrounders the exact selection.

use crate::sets::Variable;

#[derive(Debug, Clone, Copy)]
pub struct Surrounder {
    pub id: &'static str,
    pub label: &'static str,
    pub text: &'static str,
    /// Wraps the exact selection instead of whole lines.
    pub expression: bool,
    /// `(name, default value expression)` of the variables.
    pub variables: &'static [(&'static str, &'static str)],
}

impl Surrounder {
    pub fn variables(&self) -> Vec<Variable> {
        self.variables
            .iter()
            .map(|(name, default)| Variable {
                name: name.to_string(),
                expression: String::new(),
                default_value: default.to_string(),
                always_stop_at: true,
            })
            .collect()
    }
}

const fn statement(id: &'static str, label: &'static str, text: &'static str) -> Surrounder {
    Surrounder {
        id,
        label,
        text,
        expression: false,
        variables: &[],
    }
}

const fn expression(id: &'static str, label: &'static str, text: &'static str) -> Surrounder {
    Surrounder {
        id,
        label,
        text,
        expression: true,
        variables: &[],
    }
}

const PHP: &[Surrounder] = &[
    statement("php.if", "if", "if ($END$) {\n    $SELECTION$\n}"),
    statement(
        "php.if-else",
        "if / else",
        "if ($END$) {\n    $SELECTION$\n} else {\n\n}",
    ),
    statement("php.while", "while", "while ($END$) {\n    $SELECTION$\n}"),
    statement(
        "php.do-while",
        "do / while",
        "do {\n    $SELECTION$\n} while ($END$);",
    ),
    statement("php.for", "for", "for ($END$) {\n    $SELECTION$\n}"),
    statement(
        "php.foreach",
        "foreach",
        "foreach ($END$) {\n    $SELECTION$\n}",
    ),
    statement(
        "php.try-catch",
        "try / catch",
        "try {\n    $SELECTION$\n} catch (\\Exception $$e) {\n    $END$\n}",
    ),
    statement(
        "php.try-finally",
        "try / finally",
        "try {\n    $SELECTION$\n} finally {\n    $END$\n}",
    ),
    statement(
        "php.try-catch-finally",
        "try / catch / finally",
        "try {\n    $SELECTION$\n} catch (\\Exception $$e) {\n    $END$\n} finally {\n\n}",
    ),
    statement(
        "php.function",
        "function",
        "function $END$() {\n    $SELECTION$\n}",
    ),
    statement("php.braces", "{ }", "{\n    $SELECTION$\n}$END$"),
    statement(
        "php.region",
        "region...endregion Comments",
        "// region $END$\n$SELECTION$\n// endregion",
    ),
    expression("php.parentheses", "(expr)", "($SELECTION$)$END$"),
    expression("php.not", "!(expr)", "!($SELECTION$)$END$"),
];

const JS: &[Surrounder] = &[
    statement("js.if", "if", "if ($END$) {\n    $SELECTION$\n}"),
    statement(
        "js.if-else",
        "if / else",
        "if ($END$) {\n    $SELECTION$\n} else {\n\n}",
    ),
    statement("js.while", "while", "while ($END$) {\n    $SELECTION$\n}"),
    statement(
        "js.do-while",
        "do / while",
        "do {\n    $SELECTION$\n} while ($END$);",
    ),
    statement("js.for", "for", "for ($END$) {\n    $SELECTION$\n}"),
    statement(
        "js.try-catch",
        "try / catch",
        "try {\n    $SELECTION$\n} catch (e) {\n    $END$\n}",
    ),
    statement(
        "js.try-finally",
        "try / finally",
        "try {\n    $SELECTION$\n} finally {\n    $END$\n}",
    ),
    statement(
        "js.try-catch-finally",
        "try / catch / finally",
        "try {\n    $SELECTION$\n} catch (e) {\n    $END$\n} finally {\n\n}",
    ),
    statement(
        "js.function",
        "function",
        "function $END$() {\n    $SELECTION$\n}",
    ),
    statement("js.braces", "{ }", "{\n    $SELECTION$\n}$END$"),
    statement(
        "js.region",
        "region...endregion Comments",
        "//region $END$\n$SELECTION$\n//endregion",
    ),
    expression("js.parentheses", "(expr)", "($SELECTION$)$END$"),
    expression("js.not", "!(expr)", "!($SELECTION$)$END$"),
];

const LUA: &[Surrounder] = &[
    statement("lua.if", "if", "if $END$ then\n    $SELECTION$\nend"),
    statement(
        "lua.if-else",
        "if / else",
        "if $END$ then\n    $SELECTION$\nelse\n\nend",
    ),
    statement("lua.while", "while", "while $END$ do\n    $SELECTION$\nend"),
    statement(
        "lua.repeat",
        "repeat / until",
        "repeat\n    $SELECTION$\nuntil $END$",
    ),
    statement("lua.for", "for", "for $END$ do\n    $SELECTION$\nend"),
    statement(
        "lua.function",
        "function",
        "function $END$()\n    $SELECTION$\nend",
    ),
    statement("lua.do", "do ... end", "do\n    $SELECTION$\nend$END$"),
    statement(
        "lua.region",
        "region...endregion Comments",
        "--region $END$\n$SELECTION$\n--endregion",
    ),
    expression("lua.parentheses", "(expr)", "($SELECTION$)$END$"),
    expression("lua.not", "not (expr)", "not ($SELECTION$)$END$"),
];

const TAG: Surrounder = Surrounder {
    id: "html.tag",
    label: "<tag></tag>",
    text: "<$TAG$>$SELECTION$</$TAG$>",
    expression: true,
    variables: &[("TAG", "\"div\"")],
};

const HTML: &[Surrounder] = &[
    TAG,
    expression("html.comment", "<!-- -->", "<!-- $SELECTION$ -->$END$"),
];

const BLADE: &[Surrounder] = &[
    TAG,
    statement("blade.if", "@if", "@if ($END$)\n    $SELECTION$\n@endif"),
    statement(
        "blade.foreach",
        "@foreach",
        "@foreach ($END$)\n    $SELECTION$\n@endforeach",
    ),
    expression("blade.comment", "{{-- --}}", "{{-- $SELECTION$ --}}$END$"),
];

const TWIG: &[Surrounder] = &[
    TAG,
    statement(
        "twig.if",
        "{% if %}",
        "{% if $END$ %}\n    $SELECTION$\n{% endif %}",
    ),
    statement(
        "twig.for",
        "{% for %}",
        "{% for $END$ %}\n    $SELECTION$\n{% endfor %}",
    ),
    statement(
        "twig.block",
        "{% block %}",
        "{% block $END$ %}\n    $SELECTION$\n{% endblock %}",
    ),
    expression("twig.comment", "{# #}", "{# $SELECTION$ #}$END$"),
];

/// Surrounders for the contexts at the selection.
pub fn surrounders(contexts: &[String]) -> Vec<Surrounder> {
    let has = |id: &str| {
        contexts
            .iter()
            .any(|c| c == id || c.starts_with(&format!("{id}_")))
    };
    let list = if has("PHP") && !has("PHP_COMMENT") && !has("PHP_STRING") {
        PHP
    } else if has("JS") || has("TYPESCRIPT") {
        JS
    } else if has("LUA") {
        LUA
    } else if has("BLADE") {
        BLADE
    } else if has("TWIG") {
        TWIG
    } else if has("HTML") {
        HTML
    } else {
        &[]
    };
    list.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_surrounders_by_context() {
        let ids = |c: &[&str]| -> Vec<&str> {
            surrounders(&c.iter().map(|s| s.to_string()).collect::<Vec<_>>())
                .iter()
                .map(|s| s.id)
                .collect()
        };
        assert_eq!(ids(&["PHP_STATEMENT"])[0], "php.if");
        assert!(ids(&["PHP_COMMENT"]).is_empty());
        assert_eq!(ids(&["JS_STATEMENT", "TYPESCRIPT"])[0], "js.if");
        assert_eq!(ids(&["LUA"])[0], "lua.if");
        assert_eq!(ids(&["BLADE", "HTML"])[1], "blade.if");
        assert_eq!(ids(&["TWIG", "HTML"])[1], "twig.if");
        assert_eq!(ids(&["HTML"]), ["html.tag", "html.comment"]);
        assert!(ids(&["CSS"]).is_empty());
        for s in PHP
            .iter()
            .chain(JS)
            .chain(LUA)
            .chain(HTML)
            .chain(BLADE)
            .chain(TWIG)
        {
            assert!(crate::template::uses_selection(s.text), "{}", s.id);
        }
    }
}
