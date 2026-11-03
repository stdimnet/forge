//! The options handed to external formatters: settings for language servers (pushed with
//! `workspace/didChangeConfiguration` before Reformat Code) and StyLua's command line.

use serde_json::{json, Value};

use crate::format::Opts;
use crate::model::{model, Values, GENERAL};

/// `intelephense.format.braces` (read by php-lsp) for the three PHP brace options (`per` is PER Coding
/// Style, the successor of PSR-12).
pub fn intelephense_braces(
    class_next: bool,
    function_next: bool,
    other_next: bool,
) -> &'static str {
    match (class_next, function_next, other_next) {
        (true, true, true) => "allman",
        (false, false, false) => "k&r",
        (_, false, _) => "k&r",
        (_, _, true) => "allman",
        _ => "per",
    }
}

/// Settings for the language servers that format `language` (a code style language id).
/// `Value::Null` when no server takes options.
pub fn settings_patch(language: &str, values: &Values) -> Value {
    let o = Opts::new(language, values);
    let general = Opts::new(GENERAL, values);
    let width = general.int("hardWrapAt");
    let keep = o.int("blankLines.keepMax");
    match language {
        "php" => {
            json!({ "intelephense": { "format": { "enable": true, "braces": intelephense_braces(
            o.is("wrapping.classBraces", "nextLine"),
            o.is("wrapping.functionBraces", "nextLine"),
            o.is("wrapping.otherBraces", "nextLine"),
        ) } } })
        }
        "javascript" | "typescript" => {
            let mut format = json!({
                "insertSpaceAfterCommaDelimiter": o.bool("spaces.afterComma"),
                "insertSpaceAfterSemicolonInForStatements": o.bool("spaces.afterForSemicolon"),
                "insertSpaceBeforeAndAfterBinaryOperators": o.bool("spaces.aroundOperators"),
                "insertSpaceAfterKeywordsInControlFlowStatements": o.bool("spaces.beforeControlParentheses"),
                "insertSpaceAfterFunctionKeywordForAnonymousFunctions": o.bool("spaces.beforeAnonymousFunctionParentheses"),
                "insertSpaceBeforeFunctionParenthesis": o.bool("spaces.beforeFunctionDeclarationParentheses"),
                "insertSpaceAfterOpeningAndClosingNonemptyParenthesis": o.bool("spaces.withinParentheses"),
                "insertSpaceAfterOpeningAndClosingNonemptyBrackets": o.bool("spaces.withinBrackets"),
                "insertSpaceAfterOpeningAndClosingNonemptyBraces": o.bool("spaces.withinObjectLiteralBraces"),
                "placeOpenBraceOnNewLineForFunctions": o.is("wrapping.functionBraces", "nextLine"),
                "placeOpenBraceOnNewLineForControlBlocks": o.is("wrapping.otherBraces", "nextLine"),
                "semicolons": match o.str("punctuation.semicolons").as_str() {
                    "use" => "insert",
                    "dontUse" => "remove",
                    _ => "ignore",
                },
            });
            if language == "typescript" {
                format["insertSpaceBeforeTypeAnnotation"] = json!(o.bool("spaces.beforeTypeColon"));
            }
            let quotes = match o.str("punctuation.quotes").as_str() {
                "single" => "single",
                "double" => "double",
                _ => "auto",
            };
            json!({ language: { "format": format, "preferences": { "quoteStyle": quotes } } })
        }
        "css" => {
            let format = json!({
                "enable": true,
                "newlineBetweenSelectors": o.bool("wrapping.selectorPerLine"),
                "newlineBetweenRules": o.int("blankLines.aroundRule") > 0,
                "spaceAroundSelectorSeparator": o.bool("spaces.aroundSelectorCombinators"),
                "braceStyle": if o.is("wrapping.braces", "nextLine") { "expand" } else { "collapse" },
                "preserveNewLines": true,
                "maxPreserveNewLines": keep + 1,
            });
            json!({ "css": { "format": format }, "scss": { "format": format }, "less": { "format": format } })
        }
        "html" => {
            let no_indent = o.str("other.doNotIndentChildrenOf");
            json!({ "html": { "format": {
                "enable": true,
                "wrapLineLength": width,
                "wrapAttributes": match o.str("wrapping.attributes").as_str() {
                    "force" => "force",
                    "preserve" => "preserve",
                    _ => "auto",
                },
                "indentInnerHtml": !no_indent.split(',').any(|t| t.trim().eq_ignore_ascii_case("html")),
                "preserveNewLines": true,
                "maxPreserveNewLines": keep + 1,
            } } })
        }
        "json" => json!({ "json": { "format": {
            "enable": true,
            "keepLines": o.is("wrapping.objects", "asIs") && o.is("wrapping.arrays", "asIs"),
        } } }),
        "yaml" => json!({ "yaml": { "format": {
            "enable": true,
            "singleQuote": o.bool("other.singleQuote"),
            "bracketSpacing": o.bool("spaces.withinBraces"),
            "proseWrap": o.str("wrapping.proseWrap"),
            "printWidth": width,
        } } }),
        "xml" => json!({ "xml": { "format": {
            "enabled": true,
            "spaceBeforeEmptyCloseTag": o.bool("spaces.inEmptyTag"),
            "splitAttributes": o.is("wrapping.attributes", "force"),
            "preservedNewlines": keep,
            "maxLineWidth": width,
        } } }),
        "lua" => {
            let ind = o.indent();
            let quote = match o.str("other.quoteStyle").as_str() {
                "forceDouble" | "autoPreferDouble" => "double",
                _ => "single",
            };
            let parens = match o.str("other.callParentheses").as_str() {
                "noSingleString" => "remove_string_only",
                "noSingleTable" => "remove_table_only",
                "none" => "remove",
                _ => "keep",
            };
            let b = |v: bool| if v { "true" } else { "false" };
            json!({ "Lua": { "format": { "enable": true, "defaultConfig": {
                "indent_style": if ind.use_tabs { "tab" } else { "space" },
                "indent_size": ind.size.to_string(),
                "tab_width": ind.tab_size.to_string(),
                "continuation_indent": ind.continuation.to_string(),
                "max_line_length": width.to_string(),
                "quote_style": quote,
                "call_arg_parentheses": parens,
                "space_around_table_field_list": b(o.bool("spaces.withinTableBraces")),
                "space_around_math_operator": b(o.bool("spaces.aroundOperators")),
                "space_around_concat_operator": b(o.bool("spaces.aroundConcatenation")),
                "space_around_assign_operator": b(o.bool("spaces.aroundAssignment")),
                "space_after_comma": b(o.bool("spaces.afterComma")),
                "keep_line_after_if_statement": "minLine:0",
            } } } })
        }
        _ => Value::Null,
    }
}

/// StyLua's options (used when the project has no `stylua.toml`).
pub fn stylua_args(values: &Values) -> Vec<String> {
    let o = Opts::new("lua", values);
    let general = Opts::new(GENERAL, values);
    let ind = o.indent();
    let pascal = |s: &str| {
        let mut c = s.chars();
        c.next()
            .map(|f| f.to_uppercase().chain(c).collect())
            .unwrap_or_default()
    };
    let line_endings = if general.is("lineSeparator", "crlf") {
        "Windows"
    } else {
        "Unix"
    };
    vec![
        "--indent-type".into(),
        if ind.use_tabs { "Tabs" } else { "Spaces" }.into(),
        "--indent-width".into(),
        ind.size.max(1).to_string(),
        "--column-width".into(),
        general.int("hardWrapAt").to_string(),
        "--line-endings".into(),
        line_endings.into(),
        "--quote-style".into(),
        pascal(&o.str("other.quoteStyle")),
        "--call-parentheses".into(),
        pascal(&o.str("other.callParentheses")),
        "--collapse-simple-statement".into(),
        pascal(&o.str("wrapping.collapseSimpleStatements")),
    ]
}

/// Code style languages whose servers take options.
pub fn has_patch(language: &str) -> bool {
    model().language(language).is_some() && !settings_patch(language, &model().defaults()).is_null()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheme::apply_preset;

    #[test]
    fn passes_options_to_servers() {
        let v = apply_preset(&model().defaults(), "javascript", "standard").unwrap();
        let p = settings_patch("javascript", &v);
        assert_eq!(p["javascript"]["format"]["semicolons"], "remove");
        assert_eq!(
            p["javascript"]["format"]["insertSpaceBeforeFunctionParenthesis"],
            true
        );
        assert_eq!(p["javascript"]["preferences"]["quoteStyle"], "single");
        let v = apply_preset(&model().defaults(), "php", "wordpress").unwrap();
        assert_eq!(
            settings_patch("php", &v)["intelephense"]["format"]["braces"],
            "k&r"
        );
        assert_eq!(
            settings_patch("php", &model().defaults())["intelephense"]["format"]["braces"],
            "per"
        );
        assert!(settings_patch("markdown", &v).is_null());
        assert!(has_patch("lua") && !has_patch("sql"));
    }

    #[test]
    fn stylua_command_line() {
        let v = apply_preset(&model().defaults(), "lua", "twoSpaces").unwrap();
        let args = stylua_args(&v).join(" ");
        assert_eq!(
            args,
            "--indent-type Spaces --indent-width 2 --column-width 120 --line-endings Unix --quote-style AutoPreferSingle --call-parentheses Always --collapse-simple-statement Never"
        );
    }
}
