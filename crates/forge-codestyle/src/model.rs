//! The options of Settings | Editor | Code Style: the General page and one page per language,
//! each with PHPStorm's tabs ("Tabs and Indents", "Spaces", "Wrapping and Braces", "Blank
//! Lines" and language specific ones). The UI renders pages from this model; formatting,
//! presets, storage and import/export read the same definitions.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Serialize;
use serde_json::{json, Value};

/// Option values of a scheme by full key: `<language>.<option>` (`php.indent`,
/// `general.hardWrapAt`).
pub type Values = BTreeMap<String, Value>;

/// The language id of the General page.
pub const GENERAL: &str = "general";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum OptionKind {
    Bool,
    Int { min: i64, max: i64 },
    Choice { choices: Vec<Choice> },
    Text,
}

/// Where an option lives in a PHPStorm code style XML file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmlSection {
    /// `<option>` directly in `<code_scheme>`.
    Root,
    /// `<codeStyleSettings language="…">`.
    Common,
    /// `<indentOptions>` inside `<codeStyleSettings>`.
    Indent,
    /// The language's own settings element (`<PHPCodeStyleSettings>`).
    Custom,
}

/// How a value is written in PHPStorm's XML.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmlConv {
    Bool,
    Int,
    Text,
    /// `endOfLine` = 1, `nextLine` = 2.
    Brace,
    /// The line separator as an escaped string (`\n`, `\r\n`, empty for the system's).
    LineSeparator,
    /// Fixed pairs of our value and PHPStorm's.
    Map(&'static [(&'static str, &'static str)]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XmlMap {
    pub section: XmlSection,
    pub name: &'static str,
    pub conv: XmlConv,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OptionSpec {
    /// Key inside the language (`spaces.afterComma`).
    pub key: String,
    pub title: String,
    /// Tab id (`tabsAndIndents`, `spaces`, …).
    pub tab: String,
    /// Titled group inside the tab.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(flatten)]
    pub kind: OptionKind,
    pub default: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip)]
    pub xml: Option<XmlMap>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TabSpec {
    pub id: String,
    pub title: String,
}

/// A "Set from…" preset: the values it sets; the other options get their defaults.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    /// Option keys (without the language) and values.
    pub values: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LanguageSpec {
    pub id: String,
    pub name: String,
    /// File type ids (`forge-filetypes`) formatted with this language's options.
    pub file_types: Vec<String>,
    pub tabs: Vec<TabSpec>,
    pub options: Vec<OptionSpec>,
    pub presets: Vec<Preset>,
    /// A file name with the language's extension, for syntax highlighting of the preview.
    pub sample_path: String,
    /// `language` attribute of `<codeStyleSettings>` in PHPStorm's XML.
    #[serde(skip)]
    pub xml_language: &'static str,
    /// The language's own settings element in PHPStorm's XML.
    #[serde(skip)]
    pub xml_custom: &'static str,
}

impl LanguageSpec {
    pub fn option(&self, key: &str) -> Option<&OptionSpec> {
        self.options.iter().find(|o| o.key == key)
    }

    /// Every option with its default, keyed `<language>.<option>`.
    pub fn defaults(&self) -> Values {
        self.options
            .iter()
            .map(|o| (format!("{}.{}", self.id, o.key), o.default.clone()))
            .collect()
    }

    pub fn preset(&self, id: &str) -> Option<&Preset> {
        self.presets.iter().find(|p| p.id == id)
    }
}

/// Everything the Code Style pages render.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodeStyleModel {
    pub general: LanguageSpec,
    pub languages: Vec<LanguageSpec>,
}

impl CodeStyleModel {
    pub fn language(&self, id: &str) -> Option<&LanguageSpec> {
        if id == GENERAL {
            return Some(&self.general);
        }
        self.languages.iter().find(|l| l.id == id)
    }

    /// All pages: General first.
    pub fn all(&self) -> impl Iterator<Item = &LanguageSpec> {
        std::iter::once(&self.general).chain(self.languages.iter())
    }

    /// Default values of every option of every page.
    pub fn defaults(&self) -> Values {
        self.all().flat_map(LanguageSpec::defaults).collect()
    }

    /// The spec of a full key (`php.spaces.afterComma`).
    pub fn spec(&self, full_key: &str) -> Option<(&LanguageSpec, &OptionSpec)> {
        let (lang, key) = full_key.split_once('.')?;
        let l = self.language(lang)?;
        Some((l, l.option(key)?))
    }

    /// The code style language of a file type (`scss` → `css`, `tsx` → `typescript`).
    pub fn language_for_file_type(&self, file_type: &str) -> Option<&LanguageSpec> {
        self.languages
            .iter()
            .find(|l| l.file_types.iter().any(|t| t == file_type))
    }
}

/// The model (built once).
pub fn model() -> &'static CodeStyleModel {
    static MODEL: OnceLock<CodeStyleModel> = OnceLock::new();
    MODEL.get_or_init(|| CodeStyleModel {
        general: general(),
        languages: vec![
            php(),
            html(),
            css(),
            javascript(false),
            javascript(true),
            json(),
            yaml(),
            lua(),
            blade(),
            twig(),
            sql(),
            xml(),
            markdown(),
        ],
    })
}

/// Check and normalize a value for an option: clamps numbers, rejects unknown choices.
pub fn sanitize(spec: &OptionSpec, value: &Value) -> Option<Value> {
    match &spec.kind {
        OptionKind::Bool => value.as_bool().map(Value::Bool),
        OptionKind::Int { min, max } => value
            .as_i64()
            .or_else(|| value.as_f64().map(|f| f.round() as i64))
            .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
            .map(|n| json!(n.clamp(*min, *max))),
        OptionKind::Choice { choices } => value
            .as_str()
            .filter(|s| choices.iter().any(|c| c.value == *s))
            .map(|s| json!(s)),
        OptionKind::Text => value.as_str().map(|s| json!(s)),
    }
}

// ---- Builders ----------------------------------------------------------------------------

pub const TABS_AND_INDENTS: &str = "tabsAndIndents";
pub const SPACES: &str = "spaces";
pub const WRAPPING: &str = "wrappingAndBraces";
pub const BLANK_LINES: &str = "blankLines";

fn tab(id: &str) -> TabSpec {
    let title = match id {
        TABS_AND_INDENTS => "Tabs and Indents",
        SPACES => "Spaces",
        WRAPPING => "Wrapping and Braces",
        BLANK_LINES => "Blank Lines",
        "codeConversion" => "Code Conversion",
        "punctuation" => "Punctuation",
        "case" => "Case",
        "other" => "Other",
        "formatter" => "Formatter",
        "general" => "General",
        other => other,
    };
    TabSpec {
        id: id.into(),
        title: title.into(),
    }
}

struct Builder {
    tab: &'static str,
    section: Option<&'static str>,
    options: Vec<OptionSpec>,
}

impl Builder {
    fn new() -> Self {
        Self {
            tab: TABS_AND_INDENTS,
            section: None,
            options: Vec::new(),
        }
    }
    fn tab(&mut self, tab: &'static str) -> &mut Self {
        self.tab = tab;
        self.section = None;
        self
    }
    fn section(&mut self, section: &'static str) -> &mut Self {
        self.section = Some(section);
        self
    }
    fn push(&mut self, key: &str, title: &str, kind: OptionKind, default: Value) -> &mut Self {
        self.options.push(OptionSpec {
            key: key.into(),
            title: title.into(),
            tab: self.tab.into(),
            section: self.section.map(Into::into),
            kind,
            default,
            description: None,
            xml: None,
        });
        self
    }
    fn bool(&mut self, key: &str, title: &str, default: bool) -> &mut Self {
        self.push(key, title, OptionKind::Bool, json!(default))
    }
    fn int(&mut self, key: &str, title: &str, min: i64, max: i64, default: i64) -> &mut Self {
        self.push(key, title, OptionKind::Int { min, max }, json!(default))
    }
    fn choice(
        &mut self,
        key: &str,
        title: &str,
        choices: &[(&str, &str)],
        default: &str,
    ) -> &mut Self {
        let choices = choices
            .iter()
            .map(|(v, l)| Choice {
                value: (*v).into(),
                label: (*l).into(),
            })
            .collect();
        self.push(key, title, OptionKind::Choice { choices }, json!(default))
    }
    fn text(&mut self, key: &str, title: &str, default: &str) -> &mut Self {
        self.push(key, title, OptionKind::Text, json!(default))
    }
    /// Description of the last option.
    fn describe(&mut self, text: &str) -> &mut Self {
        if let Some(o) = self.options.last_mut() {
            o.description = Some(text.into());
        }
        self
    }
    /// PHPStorm XML name of the last option.
    fn xml(&mut self, section: XmlSection, name: &'static str, conv: XmlConv) -> &mut Self {
        if let Some(o) = self.options.last_mut() {
            o.xml = Some(XmlMap {
                section,
                name,
                conv,
            });
        }
        self
    }
    fn common(&mut self, name: &'static str) -> &mut Self {
        let conv = match self.options.last().map(|o| &o.kind) {
            Some(OptionKind::Int { .. }) => XmlConv::Int,
            Some(OptionKind::Choice { .. }) => XmlConv::Brace,
            Some(OptionKind::Text) => XmlConv::Text,
            _ => XmlConv::Bool,
        };
        self.xml(XmlSection::Common, name, conv)
    }
    fn custom(&mut self, name: &'static str) -> &mut Self {
        let conv = match self.options.last().map(|o| &o.kind) {
            Some(OptionKind::Int { .. }) => XmlConv::Int,
            Some(OptionKind::Text) => XmlConv::Text,
            _ => XmlConv::Bool,
        };
        self.xml(XmlSection::Custom, name, conv)
    }

    /// "Tabs and Indents" of most languages.
    fn indents(&mut self, indent: i64, continuation: Option<i64>, tabs: bool) -> &mut Self {
        self.tab(TABS_AND_INDENTS);
        if tabs {
            self.bool("useTabs", "Use tab character", false).xml(
                XmlSection::Indent,
                "USE_TAB_CHARACTER",
                XmlConv::Bool,
            );
        }
        self.int("tabSize", "Tab size", 1, 16, indent)
            .xml(XmlSection::Indent, "TAB_SIZE", XmlConv::Int)
            .int("indent", "Indent", 0, 16, indent)
            .xml(XmlSection::Indent, "INDENT_SIZE", XmlConv::Int);
        if let Some(c) = continuation {
            self.int("continuationIndent", "Continuation indent", 0, 16, c)
                .xml(XmlSection::Indent, "CONTINUATION_INDENT_SIZE", XmlConv::Int);
        }
        self.bool(
            "keepIndentsOnEmptyLines",
            "Keep indents on empty lines",
            false,
        )
        .xml(
            XmlSection::Indent,
            "KEEP_INDENTS_ON_EMPTY_LINES",
            XmlConv::Bool,
        )
    }

    fn keep_max(&mut self, default: i64) -> &mut Self {
        self.tab(BLANK_LINES)
            .section("Keep maximum blank lines")
            .int("blankLines.keepMax", "In code", 0, 10, default)
            .common("KEEP_BLANK_LINES_IN_CODE")
    }
}

const BRACES: &[(&str, &str)] = &[("endOfLine", "End of line"), ("nextLine", "Next line")];
const AS_IS: (&str, &str) = ("asIs", "Keep as is");

#[allow(clippy::too_many_arguments)]
fn language(
    id: &str,
    name: &str,
    file_types: &[&str],
    extra_tabs: &[&str],
    b: Builder,
    presets: Vec<Preset>,
    sample_path: &str,
    xml_language: &'static str,
    xml_custom: &'static str,
) -> LanguageSpec {
    let mut tabs: Vec<TabSpec> = Vec::new();
    for o in &b.options {
        if !tabs.iter().any(|t| t.id == o.tab) {
            tabs.push(tab(&o.tab));
        }
    }
    // Standard tabs first in PHPStorm's order, then the language's own.
    let order = |id: &str| {
        [TABS_AND_INDENTS, SPACES, WRAPPING, BLANK_LINES]
            .iter()
            .position(|t| *t == id)
            .unwrap_or(4 + extra_tabs.iter().position(|t| *t == id).unwrap_or(9))
    };
    tabs.sort_by_key(|t| order(&t.id));
    LanguageSpec {
        id: id.into(),
        name: name.into(),
        file_types: file_types.iter().map(|s| (*s).into()).collect(),
        tabs,
        options: b.options,
        presets,
        sample_path: sample_path.into(),
        xml_language,
        xml_custom,
    }
}

fn preset(id: &str, name: &str, values: Value) -> Preset {
    Preset {
        id: id.into(),
        name: name.into(),
        values: values
            .as_object()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default(),
    }
}

// ---- General -----------------------------------------------------------------------------

fn general() -> LanguageSpec {
    let mut b = Builder::new();
    b.tab("general")
        .choice(
            "lineSeparator",
            "Line separator",
            &[
                ("lf", "Unix and macOS (\\n)"),
                ("crlf", "Windows (\\r\\n)"),
                ("system", "System-dependent"),
            ],
            "lf",
        )
        .describe("For new files. Existing files keep theirs; change it in the status bar.")
        .xml(XmlSection::Root, "LINE_SEPARATOR", XmlConv::LineSeparator)
        .int("hardWrapAt", "Hard wrap at", 40, 400, 120)
        .describe("The right margin, drawn as a line in the editor. EditorConfig's max_line_length overrides it.")
        .xml(XmlSection::Root, "RIGHT_MARGIN", XmlConv::Int)
        .text("visualGuides", "Visual guides", "120")
        .describe("Comma-separated column numbers, e.g. 80, 120.")
        .xml(XmlSection::Root, "SOFT_MARGINS", XmlConv::Text)
        .tab("formatter")
        .bool(
            "formatterTags.enabled",
            "Turn formatter on/off with markers in code comments",
            false,
        )
        .xml(XmlSection::Root, "FORMATTER_TAGS_ENABLED", XmlConv::Bool)
        .text("formatterTags.off", "Formatter off", "@formatter:off")
        .xml(XmlSection::Root, "FORMATTER_OFF_TAG", XmlConv::Text)
        .text("formatterTags.on", "Formatter on", "@formatter:on")
        .xml(XmlSection::Root, "FORMATTER_ON_TAG", XmlConv::Text);
    let mut spec = language(
        GENERAL,
        "General",
        &[],
        &["general", "formatter"],
        b,
        Vec::new(),
        "sample.txt",
        "",
        "",
    );
    spec.tabs = vec![tab("general"), tab("formatter")];
    spec
}

// ---- PHP ---------------------------------------------------------------------------------

fn php() -> LanguageSpec {
    let mut b = Builder::new();
    b.indents(4, Some(4), true);
    b.tab(SPACES)
        .section("Before Parentheses")
        .bool(
            "spaces.beforeFunctionDeclarationParentheses",
            "Function declaration parentheses",
            false,
        )
        .common("SPACE_BEFORE_METHOD_PARENTHESES")
        .bool(
            "spaces.beforeFunctionCallParentheses",
            "Function call parentheses",
            false,
        )
        .common("SPACE_BEFORE_METHOD_CALL_PARENTHESES")
        .bool(
            "spaces.beforeAnonymousFunctionParentheses",
            "Anonymous function and arrow function parentheses",
            true,
        )
        .custom("SPACE_BEFORE_CLOSURE_LEFT_PARENTHESIS")
        .bool("spaces.beforeIfParentheses", "'if' parentheses", true)
        .common("SPACE_BEFORE_IF_PARENTHESES")
        .bool(
            "spaces.beforeForParentheses",
            "'for' and 'foreach' parentheses",
            true,
        )
        .common("SPACE_BEFORE_FOR_PARENTHESES")
        .bool("spaces.beforeWhileParentheses", "'while' parentheses", true)
        .common("SPACE_BEFORE_WHILE_PARENTHESES")
        .bool(
            "spaces.beforeSwitchParentheses",
            "'switch' and 'match' parentheses",
            true,
        )
        .common("SPACE_BEFORE_SWITCH_PARENTHESES")
        .bool("spaces.beforeCatchParentheses", "'catch' parentheses", true)
        .common("SPACE_BEFORE_CATCH_PARENTHESES")
        .section("Around Operators")
        .bool(
            "spaces.aroundAssignment",
            "Assignment operators (=, +=, …)",
            true,
        )
        .common("SPACE_AROUND_ASSIGNMENT_OPERATORS")
        .bool("spaces.aroundLogical", "Logical operators (&&, ||)", true)
        .common("SPACE_AROUND_LOGICAL_OPERATORS")
        .bool(
            "spaces.aroundEquality",
            "Equality operators (==, !=, ===)",
            true,
        )
        .common("SPACE_AROUND_EQUALITY_OPERATORS")
        .bool(
            "spaces.aroundRelational",
            "Relational operators (<, >, <=, >=, <=>)",
            true,
        )
        .common("SPACE_AROUND_RELATIONAL_OPERATORS")
        .bool("spaces.aroundAdditive", "Additive operators (+, -)", true)
        .common("SPACE_AROUND_ADDITIVE_OPERATORS")
        .bool(
            "spaces.aroundMultiplicative",
            "Multiplicative operators (*, /, %)",
            true,
        )
        .common("SPACE_AROUND_MULTIPLICATIVE_OPERATORS")
        .bool("spaces.aroundConcatenation", "Concatenation (.)", true)
        .custom("CONCAT_SPACES")
        .bool(
            "spaces.aroundNullCoalescing",
            "Null coalescing operator (??)",
            true,
        )
        .custom("SPACE_AROUND_NULL_COALESCE_OPERATOR")
        .bool("spaces.aroundKeyValue", "Key-value delimiter (=>)", true)
        .custom("SPACE_AROUND_KEY_VALUE_DELIMITER")
        .section("Within")
        .bool("spaces.withinParentheses", "Parentheses", false)
        .common("SPACE_WITHIN_PARENTHESES")
        .section("Other")
        .bool("spaces.beforeComma", "Before comma", false)
        .common("SPACE_BEFORE_COMMA")
        .bool("spaces.afterComma", "After comma", true)
        .common("SPACE_AFTER_COMMA")
        .bool("spaces.afterTypeCast", "After type cast", true)
        .common("SPACE_AFTER_TYPE_CAST")
        .bool("spaces.afterUnaryNot", "After unary not (!)", false)
        .custom("SPACE_AFTER_UNARY_NOT");
    b.tab(WRAPPING)
        .section("Braces placement")
        .choice(
            "wrapping.classBraces",
            "In class declaration",
            BRACES,
            "nextLine",
        )
        .common("CLASS_BRACE_STYLE")
        .choice(
            "wrapping.functionBraces",
            "In function declaration",
            BRACES,
            "nextLine",
        )
        .common("METHOD_BRACE_STYLE")
        .choice("wrapping.otherBraces", "Other", BRACES, "endOfLine")
        .common("BRACE_STYLE")
        .section("'if()' statement")
        .bool("wrapping.elseOnNewLine", "'else' on new line", false)
        .common("ELSE_ON_NEW_LINE")
        .section("'try' statement")
        .bool("wrapping.catchOnNewLine", "'catch' on new line", false)
        .common("CATCH_ON_NEW_LINE")
        .bool("wrapping.finallyOnNewLine", "'finally' on new line", false)
        .common("FINALLY_ON_NEW_LINE");
    b.keep_max(2)
        .section("Minimum blank lines")
        .int("blankLines.afterNamespace", "After namespace", 0, 10, 1)
        .common("BLANK_LINES_AFTER_PACKAGE")
        .int("blankLines.afterImports", "After imports", 0, 10, 1)
        .common("BLANK_LINES_AFTER_IMPORTS")
        .int("blankLines.aroundMethods", "Around method", 0, 10, 1)
        .common("BLANK_LINES_AROUND_METHOD")
        .int(
            "blankLines.beforeReturn",
            "Before return statement",
            0,
            1,
            0,
        )
        .custom("BLANK_LINE_BEFORE_RETURN_STATEMENT");
    b.tab("codeConversion")
        .choice(
            "codeConversion.arraySyntax",
            "Array declaration style",
            &[AS_IS, ("short", "Short ([…])"), ("long", "Long (array(…))")],
            "asIs",
        )
        .xml(
            XmlSection::Custom,
            "FORCE_SHORT_DECLARATION_ARRAY_STYLE",
            XmlConv::Map(&[("short", "true"), ("asIs", "false")]),
        )
        .bool(
            "codeConversion.trailingCommaInMultilineArrays",
            "Add a comma after last element in multiline array",
            false,
        )
        .custom("COMMA_AFTER_LAST_ARRAY_ELEMENT")
        .choice(
            "codeConversion.constantsCase",
            "Convert true/false/null constants to",
            &[AS_IS, ("lower", "Lower case"), ("upper", "Upper case")],
            "lower",
        )
        .xml(
            XmlSection::Custom,
            "LOWER_CASE_BOOLEAN_CONST",
            XmlConv::Map(&[("lower", "true"), ("upper", "false")]),
        );

    let psr12 = json!({
        "useTabs": false, "tabSize": 4, "indent": 4, "continuationIndent": 4,
        "wrapping.classBraces": "nextLine", "wrapping.functionBraces": "nextLine", "wrapping.otherBraces": "endOfLine",
        "spaces.aroundConcatenation": true, "spaces.afterTypeCast": true, "spaces.afterUnaryNot": false,
        "spaces.beforeAnonymousFunctionParentheses": true, "spaces.withinParentheses": false,
        "blankLines.keepMax": 2, "blankLines.afterNamespace": 1, "blankLines.afterImports": 1, "blankLines.aroundMethods": 1,
        "blankLines.beforeReturn": 0,
        "codeConversion.arraySyntax": "asIs", "codeConversion.trailingCommaInMultilineArrays": false,
        "codeConversion.constantsCase": "lower",
    });
    let mut per = psr12.clone();
    merge(
        &mut per,
        json!({ "codeConversion.arraySyntax": "short", "codeConversion.trailingCommaInMultilineArrays": true }),
    );
    let mut symfony = per.clone();
    merge(
        &mut symfony,
        json!({ "spaces.aroundConcatenation": false, "blankLines.keepMax": 1, "blankLines.beforeReturn": 1 }),
    );
    let mut laravel = symfony.clone();
    merge(&mut laravel, json!({ "spaces.afterUnaryNot": true }));
    let wordpress = json!({
        "useTabs": true, "tabSize": 4, "indent": 4, "continuationIndent": 4,
        "wrapping.classBraces": "endOfLine", "wrapping.functionBraces": "endOfLine", "wrapping.otherBraces": "endOfLine",
        "spaces.aroundConcatenation": true, "spaces.afterTypeCast": true, "spaces.afterUnaryNot": true,
        "spaces.withinParentheses": true, "spaces.beforeAnonymousFunctionParentheses": true,
        "blankLines.keepMax": 1, "blankLines.beforeReturn": 0,
        "codeConversion.arraySyntax": "long", "codeConversion.trailingCommaInMultilineArrays": true,
        "codeConversion.constantsCase": "lower",
    });
    let drupal = json!({
        "useTabs": false, "tabSize": 2, "indent": 2, "continuationIndent": 2,
        "wrapping.classBraces": "endOfLine", "wrapping.functionBraces": "endOfLine", "wrapping.otherBraces": "endOfLine",
        "spaces.aroundConcatenation": true, "spaces.afterTypeCast": true, "spaces.afterUnaryNot": false,
        "blankLines.keepMax": 1,
        "codeConversion.arraySyntax": "short", "codeConversion.trailingCommaInMultilineArrays": true,
        "codeConversion.constantsCase": "upper",
    });
    language(
        "php",
        "PHP",
        &["php"],
        &["codeConversion"],
        b,
        vec![
            preset("psr12", "PSR-12", psr12),
            preset("per2", "PER Coding Style 2.0", per),
            preset("symfony", "Symfony", symfony),
            preset("laravel", "Laravel", laravel),
            preset("wordpress", "WordPress", wordpress),
            preset("drupal", "Drupal", drupal),
        ],
        "sample.php",
        "PHP",
        "PHPCodeStyleSettings",
    )
}

fn merge(base: &mut Value, patch: Value) {
    if let (Some(b), Value::Object(p)) = (base.as_object_mut(), patch) {
        b.extend(p);
    }
}

// ---- JavaScript and TypeScript -----------------------------------------------------------

fn javascript(typescript: bool) -> LanguageSpec {
    let mut b = Builder::new();
    b.indents(4, Some(4), true);
    b.tab(SPACES)
        .section("Before Parentheses")
        .bool(
            "spaces.beforeFunctionDeclarationParentheses",
            "Function declaration parentheses",
            false,
        )
        .common("SPACE_BEFORE_METHOD_PARENTHESES")
        .bool(
            "spaces.beforeAnonymousFunctionParentheses",
            "Anonymous function parentheses",
            true,
        )
        .custom("SPACE_BEFORE_FUNCTION_LEFT_PARENTH")
        .bool(
            "spaces.beforeControlParentheses",
            "'if', 'for', 'while', 'switch' and 'catch' parentheses",
            true,
        )
        .common("SPACE_BEFORE_IF_PARENTHESES")
        .section("Around Operators")
        .bool(
            "spaces.aroundOperators",
            "Binary, assignment and arrow operators",
            true,
        )
        .common("SPACE_AROUND_ASSIGNMENT_OPERATORS")
        .section("Within")
        .bool("spaces.withinParentheses", "Parentheses", false)
        .common("SPACE_WITHIN_PARENTHESES")
        .bool("spaces.withinBrackets", "Brackets", false)
        .common("SPACE_WITHIN_BRACKETS")
        .bool(
            "spaces.withinObjectLiteralBraces",
            "Object literal and ES6 import/export braces",
            true,
        )
        .custom("SPACES_WITHIN_OBJECT_LITERAL_BRACES")
        .section("Other")
        .bool("spaces.afterComma", "After comma", true)
        .common("SPACE_AFTER_COMMA")
        .bool("spaces.afterForSemicolon", "After semicolon in 'for'", true)
        .common("SPACE_AFTER_SEMICOLON");
    if typescript {
        b.bool(
            "spaces.beforeTypeColon",
            "Before type reference colon",
            false,
        )
        .custom("SPACE_BEFORE_TYPE_COLON");
    }
    b.tab(WRAPPING)
        .section("Braces placement")
        .choice(
            "wrapping.functionBraces",
            "In functions and classes",
            BRACES,
            "endOfLine",
        )
        .common("METHOD_BRACE_STYLE")
        .choice("wrapping.otherBraces", "Other", BRACES, "endOfLine")
        .common("BRACE_STYLE");
    b.keep_max(2);
    b.tab("punctuation")
        .choice(
            "punctuation.semicolons",
            "Use semicolon to terminate statements",
            &[AS_IS, ("use", "Always"), ("dontUse", "Don't use")],
            "asIs",
        )
        .xml(
            XmlSection::Custom,
            "USE_SEMICOLON_AFTER_STATEMENT",
            XmlConv::Map(&[("use", "true"), ("dontUse", "false")]),
        )
        .choice(
            "punctuation.quotes",
            "Use quotes",
            &[AS_IS, ("single", "Single"), ("double", "Double")],
            "asIs",
        )
        .xml(
            XmlSection::Custom,
            "USE_DOUBLE_QUOTES",
            XmlConv::Map(&[("double", "true"), ("single", "false")]),
        );
    let base = json!({
        "useTabs": false, "tabSize": 2, "indent": 2, "continuationIndent": 2,
        "spaces.withinObjectLiteralBraces": true, "spaces.afterComma": true,
        "spaces.beforeAnonymousFunctionParentheses": true, "spaces.aroundOperators": true,
        "wrapping.functionBraces": "endOfLine", "wrapping.otherBraces": "endOfLine",
    });
    let mut standard = base.clone();
    merge(
        &mut standard,
        json!({ "spaces.beforeFunctionDeclarationParentheses": true, "punctuation.semicolons": "dontUse", "punctuation.quotes": "single" }),
    );
    let mut airbnb = base.clone();
    merge(
        &mut airbnb,
        json!({ "spaces.beforeFunctionDeclarationParentheses": false, "punctuation.semicolons": "use", "punctuation.quotes": "single", "blankLines.keepMax": 1 }),
    );
    let mut prettier = base;
    merge(
        &mut prettier,
        json!({ "spaces.beforeFunctionDeclarationParentheses": false, "punctuation.semicolons": "use", "punctuation.quotes": "double", "blankLines.keepMax": 1 }),
    );
    let (id, name, types, sample, xml_lang, xml_custom): (_, _, &[&str], _, _, _) = if typescript {
        (
            "typescript",
            "TypeScript",
            &["typescript", "tsx"],
            "sample.ts",
            "TypeScript",
            "TypeScriptCodeStyleSettings",
        )
    } else {
        (
            "javascript",
            "JavaScript",
            &["javascript", "jsx"],
            "sample.js",
            "JavaScript",
            "JSCodeStyleSettings",
        )
    };
    language(
        id,
        name,
        types,
        &["punctuation"],
        b,
        vec![
            preset("standard", "JavaScript Standard Style", standard),
            preset("airbnb", "Airbnb-like", airbnb),
            preset("prettier", "Prettier defaults", prettier),
        ],
        sample,
        xml_lang,
        xml_custom,
    )
}

// ---- CSS ---------------------------------------------------------------------------------

fn css() -> LanguageSpec {
    let mut b = Builder::new();
    b.indents(4, None, true);
    b.tab(SPACES)
        .bool("spaces.beforeOpeningBrace", "Before opening brace", true)
        .custom("SPACE_BEFORE_OPENING_BRACE")
        .bool("spaces.beforeColon", "Before colon", false)
        .custom("SPACE_BEFORE_COLON")
        .bool("spaces.afterColon", "After colon", true)
        .custom("SPACE_AFTER_COLON")
        .bool("spaces.afterComma", "After comma", true)
        .common("SPACE_AFTER_COMMA")
        .bool(
            "spaces.aroundSelectorCombinators",
            "Around selector combinators (>, +, ~)",
            true,
        )
        .custom("SPACE_AROUND_SELECTOR_COMBINATORS");
    b.tab(WRAPPING)
        .choice("wrapping.braces", "Brace placement", BRACES, "endOfLine")
        .custom("BRACE_PLACEMENT")
        .bool(
            "wrapping.selectorPerLine",
            "Each selector on a new line",
            true,
        )
        .custom("KEEP_SINGLE_LINE_BLOCKS");
    b.keep_max(1)
        .section("Minimum blank lines")
        .int("blankLines.aroundRule", "Around rule", 0, 10, 1)
        .custom("BLANK_LINES_AROUND_NESTED_SELECTOR");
    b.tab("other")
        .choice(
            "other.hexColorCase",
            "Convert hex colors to",
            &[AS_IS, ("lower", "Lower case"), ("upper", "Upper case")],
            "asIs",
        )
        .xml(
            XmlSection::Custom,
            "HEX_COLOR_UPPER_CASE",
            XmlConv::Map(&[("upper", "true"), ("lower", "false")]),
        );
    let stylelint =
        json!({ "useTabs": false, "tabSize": 2, "indent": 2, "other.hexColorCase": "lower" });
    language(
        "css",
        "CSS/SCSS/Less",
        &["css", "scss", "less", "sass", "stylus"],
        &["other"],
        b,
        vec![preset("stylelint", "stylelint-config-standard", stylelint)],
        "sample.scss",
        "CSS",
        "CssCodeStyleSettings",
    )
}

// ---- JSON --------------------------------------------------------------------------------

fn json() -> LanguageSpec {
    let mut b = Builder::new();
    b.indents(2, None, true);
    b.tab(SPACES)
        .bool("spaces.beforeColon", "Before colon", false)
        .custom("SPACE_BEFORE_COLON")
        .bool("spaces.afterColon", "After colon", true)
        .custom("SPACE_AFTER_COLON")
        .bool("spaces.afterComma", "After comma", true)
        .common("SPACE_AFTER_COMMA")
        .bool("spaces.withinBraces", "Within braces", false)
        .common("SPACE_WITHIN_BRACES")
        .bool("spaces.withinBrackets", "Within brackets", false)
        .common("SPACE_WITHIN_BRACKETS");
    const WRAP: &[(&str, &str)] = &[("always", "Wrap always"), ("asIs", "Keep line breaks")];
    b.tab(WRAPPING)
        .choice("wrapping.objects", "Objects", WRAP, "always")
        .xml(
            XmlSection::Custom,
            "OBJECT_WRAPPING",
            XmlConv::Map(&[("always", "2"), ("asIs", "0")]),
        )
        .choice("wrapping.arrays", "Arrays", WRAP, "always")
        .xml(
            XmlSection::Custom,
            "ARRAY_WRAPPING",
            XmlConv::Map(&[("always", "2"), ("asIs", "0")]),
        );
    b.keep_max(0);
    language(
        "json",
        "JSON",
        &["json"],
        &[],
        b,
        Vec::new(),
        "sample.json",
        "JSON",
        "JsonCodeStyleSettings",
    )
}

// ---- YAML --------------------------------------------------------------------------------

fn yaml() -> LanguageSpec {
    let mut b = Builder::new();
    b.indents(2, None, false)
        .bool("indentSequenceValue", "Indent sequence value", true)
        .custom("INDENT_SEQUENCE_VALUE");
    b.tab(SPACES)
        .bool("spaces.withinBraces", "Within braces", true)
        .common("SPACE_WITHIN_BRACES")
        .bool("spaces.withinBrackets", "Within brackets", false)
        .common("SPACE_WITHIN_BRACKETS");
    b.tab(WRAPPING)
        .choice(
            "wrapping.proseWrap",
            "Wrap long text",
            &[
                ("preserve", "Keep as is"),
                ("always", "Wrap at the right margin"),
                ("never", "Never wrap"),
            ],
            "preserve",
        )
        .xml(
            XmlSection::Custom,
            "PROSE_WRAP",
            XmlConv::Map(&[("preserve", "0"), ("always", "1"), ("never", "2")]),
        );
    b.keep_max(2);
    b.tab("other")
        .bool("other.singleQuote", "Prefer single quotes", false)
        .custom("USE_SINGLE_QUOTES");
    language(
        "yaml",
        "YAML",
        &["yaml", "docker-compose", "symfony-services"],
        &["other"],
        b,
        Vec::new(),
        "sample.yaml",
        "yaml",
        "YAMLCodeStyleSettings",
    )
}

// ---- Lua ---------------------------------------------------------------------------------

fn lua() -> LanguageSpec {
    let mut b = Builder::new();
    b.indents(4, Some(4), true);
    b.tab(SPACES)
        .section("Around Operators")
        .bool("spaces.aroundAssignment", "Assignment operator (=)", true)
        .common("SPACE_AROUND_ASSIGNMENT_OPERATORS")
        .bool(
            "spaces.aroundOperators",
            "Math and comparison operators",
            true,
        )
        .common("SPACE_AROUND_ADDITIVE_OPERATORS")
        .bool("spaces.aroundConcatenation", "Concatenation (..)", true)
        .custom("SPACE_AROUND_CONCAT_OPERATOR")
        .section("Other")
        .bool("spaces.afterComma", "After comma", true)
        .common("SPACE_AFTER_COMMA")
        .bool("spaces.withinTableBraces", "Within table braces", true)
        .common("SPACE_WITHIN_BRACES");
    b.tab(WRAPPING).choice(
        "wrapping.collapseSimpleStatements",
        "Keep simple statements on one line",
        &[
            ("never", "Never"),
            ("functionOnly", "Functions only"),
            ("conditionalOnly", "Conditionals only"),
            ("always", "Always"),
        ],
        "never",
    );
    b.keep_max(2);
    b.tab("other")
        .section("StyLua and lua-language-server")
        .choice(
            "other.quoteStyle",
            "Quotes",
            &[
                ("autoPreferDouble", "Prefer double quotes"),
                ("autoPreferSingle", "Prefer single quotes"),
                ("forceDouble", "Always double quotes"),
                ("forceSingle", "Always single quotes"),
            ],
            "autoPreferDouble",
        )
        .choice(
            "other.callParentheses",
            "Parentheses in calls",
            &[
                ("always", "Always"),
                ("noSingleString", "Omit for a single string argument"),
                ("noSingleTable", "Omit for a single table argument"),
                ("none", "Omit for a single string or table argument"),
            ],
            "always",
        );
    let stylua = json!({ "useTabs": true, "tabSize": 4, "indent": 4, "continuationIndent": 4,
        "other.quoteStyle": "autoPreferDouble", "other.callParentheses": "always",
        "wrapping.collapseSimpleStatements": "never", "spaces.withinTableBraces": true });
    let two = json!({ "useTabs": false, "tabSize": 2, "indent": 2, "continuationIndent": 2,
        "other.quoteStyle": "autoPreferSingle" });
    language(
        "lua",
        "Lua",
        &["lua"],
        &["other"],
        b,
        vec![
            preset("stylua", "StyLua defaults", stylua),
            preset("twoSpaces", "2 spaces, single quotes", two),
        ],
        "sample.lua",
        "Lua",
        "LuaCodeStyleSettings",
    )
}

// ---- Markup ------------------------------------------------------------------------------

fn markup_spaces(b: &mut Builder, prefix: &'static str) {
    let (eq, empty): (&'static str, &'static str) = if prefix == "XML" {
        (
            "XML_SPACE_AROUND_EQUALITY_IN_ATTRIBUTE",
            "XML_SPACE_INSIDE_EMPTY_TAG",
        )
    } else {
        (
            "HTML_SPACE_AROUND_EQUALITY_IN_ATTRIBUTE",
            "HTML_SPACE_INSIDE_EMPTY_TAG",
        )
    };
    b.tab(SPACES)
        .bool(
            "spaces.aroundAttributeEquals",
            "Around \"=\" in attribute",
            false,
        )
        .custom(eq)
        .bool("spaces.inEmptyTag", "In empty tag", false)
        .custom(empty);
}

fn html() -> LanguageSpec {
    let mut b = Builder::new();
    b.indents(4, Some(8), true);
    markup_spaces(&mut b, "HTML");
    b.tab(WRAPPING)
        .choice(
            "wrapping.attributes",
            "Wrap attributes",
            &[
                ("auto", "Wrap if long"),
                ("force", "Chop down always"),
                ("preserve", "Keep line breaks"),
            ],
            "auto",
        )
        .xml(
            XmlSection::Custom,
            "HTML_ATTRIBUTE_WRAP",
            XmlConv::Map(&[("auto", "1"), ("force", "2"), ("preserve", "0")]),
        );
    b.keep_max(2);
    b.tab("other")
        .text(
            "other.doNotIndentChildrenOf",
            "Do not indent children of",
            "html,body,thead,tbody,tfoot",
        )
        .describe("Comma-separated tag names.")
        .custom("HTML_DO_NOT_INDENT_CHILDREN_OF");
    language(
        "html",
        "HTML",
        &["html", "vue"],
        &["other"],
        b,
        Vec::new(),
        "sample.html",
        "HTML",
        "HTMLCodeStyleSettings",
    )
}

fn xml() -> LanguageSpec {
    let mut b = Builder::new();
    b.indents(4, Some(8), true);
    markup_spaces(&mut b, "XML");
    b.tab(WRAPPING)
        .choice(
            "wrapping.attributes",
            "Wrap attributes",
            &[
                ("preserve", "Keep line breaks"),
                ("force", "Chop down always"),
            ],
            "preserve",
        )
        .xml(
            XmlSection::Custom,
            "XML_ATTRIBUTE_WRAP",
            XmlConv::Map(&[("preserve", "0"), ("force", "2")]),
        );
    b.keep_max(2);
    language(
        "xml",
        "XML",
        &["xml"],
        &[],
        b,
        Vec::new(),
        "sample.xml",
        "XML",
        "XmlCodeStyleSettings",
    )
}

fn blade() -> LanguageSpec {
    let mut b = Builder::new();
    b.indents(4, Some(8), true);
    b.tab(SPACES)
        .bool(
            "spaces.withinEchoBraces",
            "Within echo braces ({{ }}, {!! !!})",
            true,
        )
        .custom("SPACES_WITHIN_ECHO_BRACES")
        .bool(
            "spaces.beforeDirectiveParentheses",
            "Before directive parentheses (@if (…))",
            false,
        )
        .custom("SPACE_BEFORE_DIRECTIVE_PARENTHESES");
    b.keep_max(2);
    language(
        "blade",
        "Blade",
        &["blade"],
        &[],
        b,
        Vec::new(),
        "sample.blade.php",
        "Blade",
        "BladeCodeStyleSettings",
    )
}

fn twig() -> LanguageSpec {
    let mut b = Builder::new();
    b.indents(4, Some(8), true);
    b.tab(SPACES)
        .bool(
            "spaces.withinDelimiters",
            "Within delimiters ({{ }}, {% %})",
            true,
        )
        .custom("SPACES_INSIDE_DELIMITERS");
    b.keep_max(2);
    language(
        "twig",
        "Twig",
        &["twig"],
        &[],
        b,
        Vec::new(),
        "sample.twig",
        "Twig",
        "TwigCodeStyleSettings",
    )
}

// ---- SQL ---------------------------------------------------------------------------------

fn sql() -> LanguageSpec {
    let mut b = Builder::new();
    b.indents(4, Some(4), true);
    b.tab(SPACES)
        .bool("spaces.afterComma", "After comma", true)
        .common("SPACE_AFTER_COMMA")
        .bool("spaces.aroundOperators", "Around operators", true)
        .common("SPACE_AROUND_RELATIONAL_OPERATORS")
        .bool("spaces.withinParentheses", "Within parentheses", false)
        .common("SPACE_WITHIN_PARENTHESES");
    b.keep_max(2);
    b.tab("case")
        .choice(
            "case.keywords",
            "Keywords",
            &[AS_IS, ("upper", "Upper case"), ("lower", "Lower case")],
            "asIs",
        )
        .xml(
            XmlSection::Custom,
            "KEYWORD_CASE",
            XmlConv::Map(&[("asIs", "0"), ("upper", "1"), ("lower", "2")]),
        );
    language(
        "sql",
        "SQL",
        &["sql"],
        &["case"],
        b,
        vec![preset(
            "upper",
            "Upper case keywords",
            json!({ "case.keywords": "upper", "indent": 4, "continuationIndent": 4 }),
        )],
        "sample.sql",
        "SQL",
        "SqlCodeStyleSettings",
    )
}

// ---- Markdown ----------------------------------------------------------------------------

fn markdown() -> LanguageSpec {
    let mut b = Builder::new();
    b.indents(4, None, true);
    b.tab(SPACES)
        .bool(
            "spaces.afterHeaderSymbol",
            "Force one space after header symbol",
            true,
        )
        .custom("FORCE_ONE_SPACE_AFTER_HEADER_SYMBOL")
        .bool(
            "spaces.afterListMarker",
            "Force one space after list bullet",
            true,
        )
        .custom("FORCE_ONE_SPACE_AFTER_LIST_BULLET")
        .bool(
            "spaces.afterBlockquoteMarker",
            "Force one space after blockquote symbol",
            true,
        )
        .custom("FORCE_ONE_SPACE_AFTER_BLOCKQUOTE_SYMBOL");
    b.tab(BLANK_LINES)
        .section("Maximum blank lines")
        .int("blankLines.keepMax", "Between paragraphs", 0, 10, 1)
        .custom("MAX_LINES_BETWEEN_PARAGRAPHS")
        .section("Minimum blank lines")
        .int("blankLines.aroundHeader", "Around header", 0, 10, 1)
        .custom("MIN_LINES_AROUND_HEADER")
        .int(
            "blankLines.aroundBlockElements",
            "Around block elements (code blocks, tables, quotes)",
            0,
            10,
            1,
        )
        .custom("MIN_LINES_AROUND_BLOCK_ELEMENTS");
    language(
        "markdown",
        "Markdown",
        &["markdown"],
        &[],
        b,
        Vec::new(),
        "sample.md",
        "Markdown",
        "MarkdownCodeStyleSettings",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_the_standard_tabs_and_valid_presets() {
        let m = model();
        assert_eq!(m.languages.len(), 13);
        for l in &m.languages {
            assert_eq!(l.tabs[0].id, TABS_AND_INDENTS, "{}", l.id);
            assert!(l.option("indent").is_some(), "{}", l.id);
            assert!(l.option("blankLines.keepMax").is_some(), "{}", l.id);
            let mut keys: Vec<&str> = l.options.iter().map(|o| o.key.as_str()).collect();
            keys.sort_unstable();
            keys.dedup();
            assert_eq!(keys.len(), l.options.len(), "duplicate option in {}", l.id);
            for o in &l.options {
                assert_eq!(
                    sanitize(o, &o.default).as_ref(),
                    Some(&o.default),
                    "{}.{}",
                    l.id,
                    o.key
                );
                assert!(l.tabs.iter().any(|t| t.id == o.tab));
            }
            for p in &l.presets {
                for (k, v) in &p.values {
                    let spec = l
                        .option(k)
                        .unwrap_or_else(|| panic!("{}.{} in preset {}", l.id, k, p.id));
                    assert_eq!(
                        sanitize(spec, v).as_ref(),
                        Some(v),
                        "{}.{} in {}",
                        l.id,
                        k,
                        p.id
                    );
                }
            }
        }
        let php = m.language("php").unwrap();
        let names: Vec<&str> = php.presets.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "PSR-12",
                "PER Coding Style 2.0",
                "Symfony",
                "Laravel",
                "WordPress",
                "Drupal"
            ]
        );
        assert_eq!(
            php.tabs
                .iter()
                .map(|t| t.title.as_str())
                .collect::<Vec<_>>(),
            [
                "Tabs and Indents",
                "Spaces",
                "Wrapping and Braces",
                "Blank Lines",
                "Code Conversion"
            ]
        );
    }

    #[test]
    fn maps_file_types_and_keys() {
        let m = model();
        assert_eq!(m.language_for_file_type("scss").unwrap().id, "css");
        assert_eq!(m.language_for_file_type("tsx").unwrap().id, "typescript");
        assert!(m.language_for_file_type("python").is_none());
        let (l, o) = m.spec("php.spaces.afterComma").unwrap();
        assert_eq!((l.id.as_str(), o.title.as_str()), ("php", "After comma"));
        assert!(m.spec("general.hardWrapAt").is_some());
        assert!(m.spec("php.nope").is_none());
        assert_eq!(m.defaults()["json.indent"], json!(2));
    }

    #[test]
    fn sanitizes_values() {
        let (_, o) = model().spec("php.indent").unwrap();
        assert_eq!(sanitize(o, &json!(99)), Some(json!(16)));
        assert_eq!(sanitize(o, &json!("3")), Some(json!(3)));
        let (_, o) = model().spec("php.wrapping.classBraces").unwrap();
        assert_eq!(sanitize(o, &json!("sideways")), None);
    }
}
