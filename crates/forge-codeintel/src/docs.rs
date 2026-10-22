//! Documentation actions of the View menu, like PHPStorm:
//!
//! - External Documentation (Shift+F1): [`symbol_at`] describes the symbol at the caret and
//!   [`external_doc_url`] picks its page on php.net, the Laravel API, MDN, the Vue and
//!   React references or the Lua manual.
//! - Quick Definition (Ctrl+Shift+I): [`definition_snippet`] cuts a declaration out of its
//!   file, with its docblock and attributes, up to the matching brace (or Lua `end`).
//! - Context Info (Alt+Q): [`context_headers`] lists the headers of the classes, functions
//!   and rules that enclose the caret.
//! - Type Info (Ctrl+Shift+P): [`type_from_hover`] reads the type out of a language
//!   server's hover, [`type_from_signature`] out of a declaration's signature.
//!
//! Lines are 0-based; columns are UTF-16 code units like LSP positions.

use serde::{Deserialize, Serialize};

use crate::edit::Position;

/// The language id of a file by its extension (`php`, `typescript`, `css`, `lua`, ...);
/// `plaintext` for everything else.
pub fn language_of(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".blade.php") {
        return "blade";
    }
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match ext {
        "php" | "phtml" | "inc" => "php",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "javascriptreact",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "typescriptreact",
        "vue" => "vue",
        "css" => "css",
        "scss" => "scss",
        "less" => "less",
        "html" | "htm" => "html",
        "twig" => "twig",
        "lua" => "lua",
        _ => "plaintext",
    }
}

fn is_script(language: &str) -> bool {
    matches!(
        language,
        "javascript" | "javascriptreact" | "typescript" | "typescriptreact" | "vue"
    )
}

fn is_css(language: &str) -> bool {
    matches!(language, "css" | "scss" | "less")
}

fn is_markup(language: &str) -> bool {
    matches!(language, "html" | "vue" | "blade" | "twig")
}

/// Languages whose blocks are delimited by braces.
fn has_braces(language: &str) -> bool {
    language == "php" || is_script(language) || is_css(language)
}

// ---- Lines and columns ----

pub(crate) fn lines_of(text: &str) -> Vec<&str> {
    text.split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect()
}

/// The byte index of a UTF-16 column in `line` (clamped to its end).
pub(crate) fn byte_col(line: &str, utf16: u32) -> usize {
    let mut units = 0u32;
    for (i, ch) in line.char_indices() {
        if units >= utf16 {
            return i;
        }
        units += ch.len_utf16() as u32;
    }
    line.len()
}

// ---- External Documentation ----

/// Where a symbol is declared, as Go to Declaration found it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocTarget {
    /// Workspace-relative, or `library://…` for the PHP stubs and other read-only files.
    pub path: String,
    /// `strlen()`, `DateTime::format()`, `Throwable`; empty when unknown.
    #[serde(default)]
    pub name: String,
    /// Namespace, declaring class or stub extension.
    #[serde(default)]
    pub container: Option<String>,
}

/// The symbol at the caret, as far as the text tells.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocSymbol {
    pub language: String,
    /// The identifier at the caret, without PHP's `$`.
    pub word: String,
    /// The receiver before `.`, `::`, `->` or Lua's `:` (`JSON` in `JSON.parse`).
    #[serde(default)]
    pub qualifier: Option<String>,
    /// Followed by `(`.
    #[serde(default)]
    pub call: bool,
    /// A tag name (`<div`, `</div`).
    #[serde(default)]
    pub tag: bool,
    /// A CSS property name (`color: red`).
    #[serde(default)]
    pub css_property: bool,
    /// A CSS at-rule (`@media`) or pseudo-class (`:hover`): the prefix.
    #[serde(default)]
    pub css_prefix: Option<String>,
    /// Frameworks the file imports from (`vue`, `react`).
    #[serde(default)]
    pub imports: Vec<String>,
    /// The resolved declaration, filled in by the caller.
    #[serde(default)]
    pub declaration: Option<DocTarget>,
}

fn is_ident(ch: char, language: &str) -> bool {
    ch.is_alphanumeric()
        || ch == '_'
        || (ch == '$' && (language == "php" || is_script(language)))
        || (ch == '-' && (is_css(language) || is_markup(language)))
}

/// The symbol at `pos` (on or right after an identifier), or `None` off identifiers.
pub fn symbol_at(text: &str, pos: Position, language: &str) -> Option<DocSymbol> {
    let lines = lines_of(text);
    let line = *lines.get(pos.line as usize)?;
    let col = byte_col(line, pos.character);
    let ident = |c: char| is_ident(c, language);
    let mut start = col;
    while let Some(c) = line[..start].chars().next_back().filter(|c| ident(*c)) {
        start -= c.len_utf8();
    }
    let mut end = col;
    while let Some(c) = line[end..].chars().next().filter(|c| ident(*c)) {
        end += c.len_utf8();
    }
    // `$` is part of PHP variables and JS identifiers, but only at their start.
    let raw = &line[start..end];
    let word = raw.trim_start_matches('$').trim_matches('-');
    if word.is_empty() || word.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let before = line[..start].trim_end();
    let after = line[end..].trim_start();

    let mut qualifier = None;
    for sep in ["?->", "->", "::", ".", ":"] {
        if sep == ":" && language != "lua" {
            continue;
        }
        if let Some(rest) = before.strip_suffix(sep) {
            let rest = rest.trim_end();
            let q: String = rest
                .chars()
                .rev()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$' || *c == '\\')
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            if !q.is_empty() {
                qualifier = Some(q);
            }
            break;
        }
    }

    let tag = is_markup(language) && (before.ends_with('<') || before.ends_with("</"));
    let mut css_property = false;
    let mut css_prefix = None;
    if is_css(language) {
        if before.ends_with('@') {
            css_prefix = Some("@".to_string());
        } else if before.ends_with(':')
            && !before.ends_with(": ")
            && (after.contains('{') || after.ends_with(',') || after.is_empty())
            && !line[..start].ends_with(' ')
        {
            let colons = if before.ends_with("::") { "::" } else { ":" };
            css_prefix = Some(colons.to_string());
        } else if after.starts_with(':')
            && (before.is_empty() || before.ends_with('{') || before.ends_with(';'))
        {
            css_property = true;
        }
    }

    let mut imports = Vec::new();
    for fw in ["vue", "react"] {
        let named = [
            format!("from '{fw}'"),
            format!("from \"{fw}\""),
            format!("require('{fw}')"),
            format!("require(\"{fw}\")"),
        ];
        if (fw == "vue" && language == "vue") || named.iter().any(|n| text.contains(n.as_str())) {
            imports.push(fw.to_string());
        }
    }

    Some(DocSymbol {
        language: language.to_string(),
        word: word.to_string(),
        qualifier,
        call: after.starts_with('('),
        tag,
        css_property,
        css_prefix,
        imports,
        declaration: None,
    })
}

const PHP_MANUAL: &str = "https://www.php.net/manual/en/";
const MDN: &str = "https://developer.mozilla.org/en-US/docs/Web/";
const LUA_MANUAL: &str = "https://www.lua.org/manual/5.4/manual.html";

/// The page External Documentation opens for `sym`, or `None` when there is none (project
/// symbols, unknown names).
pub fn external_doc_url(sym: &DocSymbol) -> Option<String> {
    let lang = sym.language.as_str();
    if sym.tag {
        return html_url(&sym.word);
    }
    match lang {
        "php" => php_url(sym),
        "lua" => lua_url(sym),
        l if is_css(l) => css_url(sym),
        l if is_script(l) => framework_url(sym).or_else(|| js_url(sym)),
        _ => None,
    }
}

/// php.net's page name for a function or method name: lower case, `_` as `-`.
fn php_page(name: &str) -> String {
    name.trim_start_matches("__")
        .to_ascii_lowercase()
        .replace('_', "-")
}

fn php_class_url(class: &str) -> String {
    let class = class.trim_start_matches('\\').to_ascii_lowercase();
    format!("{PHP_MANUAL}class.{}.php", class.replace('\\', "-"))
}

fn php_keyword_page(word: &str) -> Option<&'static str> {
    Some(match word {
        "if" => "control-structures.if.php",
        "else" => "control-structures.else.php",
        "elseif" => "control-structures.elseif.php",
        "while" => "control-structures.while.php",
        "do" => "control-structures.do.while.php",
        "for" => "control-structures.for.php",
        "foreach" => "control-structures.foreach.php",
        "break" => "control-structures.break.php",
        "continue" => "control-structures.continue.php",
        "switch" | "case" | "default" => "control-structures.switch.php",
        "match" => "control-structures.match.php",
        "declare" => "control-structures.declare.php",
        "return" => "function.return.php",
        "require" => "function.require.php",
        "include" => "function.include.php",
        "require_once" => "function.require-once.php",
        "include_once" => "function.include-once.php",
        "goto" => "control-structures.goto.php",
        "function" => "functions.user-defined.php",
        "fn" => "functions.arrow.php",
        "class" | "new" | "extends" => "language.oop5.basic.php",
        "interface" | "implements" => "language.oop5.interfaces.php",
        "trait" => "language.oop5.traits.php",
        "enum" => "language.enumerations.php",
        "abstract" => "language.oop5.abstract.php",
        "static" => "language.oop5.static.php",
        "namespace" | "use" => "language.namespaces.php",
        "try" | "catch" | "finally" | "throw" => "language.exceptions.php",
        "yield" => "language.generators.syntax.php",
        "instanceof" => "language.operators.type.php",
        "const" => "language.constants.syntax.php",
        "echo" => "function.echo.php",
        "print" => "function.print.php",
        "isset" => "function.isset.php",
        "unset" => "function.unset.php",
        "empty" => "function.empty.php",
        "list" => "function.list.php",
        "array" => "language.types.array.php",
        "exit" | "die" => "function.exit.php",
        "eval" => "function.eval.php",
        _ => return None,
    })
}

/// `vendor/laravel/framework/src/Illuminate/Support/Collection.php` → `Illuminate\Support\Collection`.
fn laravel_class(path: &str) -> Option<String> {
    let i = path.find("Illuminate/")?;
    let class = path[i..].strip_suffix(".php")?;
    Some(class.replace('/', "\\"))
}

fn is_builtin_path(path: &str) -> bool {
    path.starts_with("library://php-stubs/")
        || path.contains("/stubs/")
        || path.contains("phpstorm-stubs")
}

fn php_url(sym: &DocSymbol) -> Option<String> {
    let Some(decl) = &sym.declaration else {
        return php_keyword_page(&sym.word.to_ascii_lowercase())
            .map(|p| format!("{PHP_MANUAL}{p}"));
    };
    if let Some(class) = laravel_class(&decl.path) {
        let short = class.rsplit('\\').next().unwrap_or(&class);
        let mut url = format!(
            "https://laravel.com/api/master/{}.html",
            class.replace('\\', "/")
        );
        if sym.word != short && sym.call {
            url.push_str(&format!("#method_{}", sym.word));
        }
        return Some(url);
    }
    if !is_builtin_path(&decl.path) {
        return None;
    }
    let name = decl.name.trim_start_matches('\\');
    if let Some((class, member)) = name.split_once("::") {
        return Some(match member.strip_suffix("()") {
            Some(method) => format!(
                "{PHP_MANUAL}{}.{}.php",
                class.to_ascii_lowercase().replace('\\', "-"),
                php_page(method)
            ),
            None => php_class_url(class),
        });
    }
    if let Some(function) = name.strip_suffix("()") {
        return Some(format!("{PHP_MANUAL}function.{}.php", php_page(function)));
    }
    if !name.is_empty() {
        return Some(if name.chars().any(|c| c.is_lowercase()) {
            php_class_url(name)
        } else {
            // Global constants are documented on their extension's page.
            format!("{PHP_MANUAL}reserved.constants.php")
        });
    }
    // A language server's target without a name: the caret's word says what it is.
    match (&sym.qualifier, sym.call) {
        (None, true) => Some(format!("{PHP_MANUAL}function.{}.php", php_page(&sym.word))),
        (Some(q), true) if q.chars().next().is_some_and(char::is_uppercase) => Some(format!(
            "{PHP_MANUAL}{}.{}.php",
            q.trim_start_matches('\\')
                .to_ascii_lowercase()
                .replace('\\', "-"),
            php_page(&sym.word)
        )),
        (None, false) => Some(php_class_url(&sym.word)),
        _ => None,
    }
}

const JS_GLOBALS: &[&str] = &[
    "Array",
    "ArrayBuffer",
    "BigInt",
    "Boolean",
    "DataView",
    "Date",
    "Error",
    "EvalError",
    "Float32Array",
    "Float64Array",
    "Function",
    "Infinity",
    "Int8Array",
    "Int16Array",
    "Int32Array",
    "Intl",
    "JSON",
    "Map",
    "Math",
    "NaN",
    "Number",
    "Object",
    "Promise",
    "Proxy",
    "RangeError",
    "ReferenceError",
    "Reflect",
    "RegExp",
    "Set",
    "String",
    "Symbol",
    "SyntaxError",
    "TypeError",
    "URIError",
    "Uint8Array",
    "Uint16Array",
    "Uint32Array",
    "WeakMap",
    "WeakRef",
    "WeakSet",
    "decodeURI",
    "decodeURIComponent",
    "encodeURI",
    "encodeURIComponent",
    "globalThis",
    "isFinite",
    "isNaN",
    "parseFloat",
    "parseInt",
    "undefined",
];

const WEB_INTERFACES: &[&str] = &[
    "AbortController",
    "Blob",
    "CustomEvent",
    "Document",
    "Element",
    "Event",
    "EventTarget",
    "File",
    "FileReader",
    "FormData",
    "Headers",
    "History",
    "HTMLElement",
    "HTMLInputElement",
    "IntersectionObserver",
    "KeyboardEvent",
    "Location",
    "MouseEvent",
    "MutationObserver",
    "Navigator",
    "Node",
    "Request",
    "ResizeObserver",
    "Response",
    "Storage",
    "URL",
    "URLSearchParams",
    "WebSocket",
    "Window",
    "Worker",
];

/// Global functions and objects of `window` documented on the Window page.
const WINDOW_MEMBERS: &[&str] = &[
    "alert",
    "cancelAnimationFrame",
    "clearInterval",
    "clearTimeout",
    "confirm",
    "fetch",
    "localStorage",
    "prompt",
    "queueMicrotask",
    "requestAnimationFrame",
    "sessionStorage",
    "setInterval",
    "setTimeout",
    "structuredClone",
];

/// Instances with their own interface page: `document` → `Document`.
fn web_instance(word: &str) -> Option<&'static str> {
    Some(match word {
        "document" => "Document",
        "window" => "Window",
        "navigator" => "Navigator",
        "location" => "Location",
        "history" => "History",
        "console" => "console",
        "crypto" => "Crypto",
        "performance" => "Performance",
        _ => return None,
    })
}

fn js_keyword_page(word: &str) -> Option<&'static str> {
    Some(match word {
        "if" | "else" => "Statements/if...else",
        "for" => "Statements/for",
        "while" => "Statements/while",
        "do" => "Statements/do...while",
        "switch" | "case" | "default" => "Statements/switch",
        "return" => "Statements/return",
        "const" => "Statements/const",
        "let" => "Statements/let",
        "var" => "Statements/var",
        "function" => "Statements/function",
        "class" | "extends" => "Statements/class",
        "try" | "catch" | "finally" => "Statements/try...catch",
        "throw" => "Statements/throw",
        "import" => "Statements/import",
        "export" => "Statements/export",
        "break" => "Statements/break",
        "continue" => "Statements/continue",
        "debugger" => "Statements/debugger",
        "async" => "Statements/async_function",
        "await" => "Operators/await",
        "new" => "Operators/new",
        "typeof" => "Operators/typeof",
        "instanceof" => "Operators/instanceof",
        "this" => "Operators/this",
        "yield" => "Operators/yield",
        "delete" => "Operators/delete",
        _ => return None,
    })
}

fn js_url(sym: &DocSymbol) -> Option<String> {
    let w = sym.word.as_str();
    let js = format!("{MDN}JavaScript/Reference/");
    if let Some(q) = sym.qualifier.as_deref() {
        if JS_GLOBALS.contains(&q) {
            return Some(format!("{js}Global_Objects/{q}/{w}"));
        }
        if q == "console" {
            return Some(format!("{MDN}API/console/{w}_static"));
        }
        if let Some(i) = web_instance(q) {
            return Some(format!("{MDN}API/{i}/{w}"));
        }
        if WEB_INTERFACES.contains(&q) {
            return Some(format!("{MDN}API/{q}/{w}"));
        }
        return None;
    }
    if JS_GLOBALS.contains(&w) {
        return Some(format!("{js}Global_Objects/{w}"));
    }
    if WEB_INTERFACES.contains(&w) {
        return Some(format!("{MDN}API/{w}"));
    }
    if let Some(i) = web_instance(w) {
        return Some(format!("{MDN}API/{i}"));
    }
    if WINDOW_MEMBERS.contains(&w) {
        return Some(format!("{MDN}API/Window/{w}"));
    }
    js_keyword_page(w).map(|p| format!("{js}{p}"))
}

/// Vue's Composition API and React's hooks, in files that import them.
fn framework_url(sym: &DocSymbol) -> Option<String> {
    if sym.qualifier.is_some() {
        return None;
    }
    let w = sym.word.as_str();
    if sym.imports.iter().any(|i| i == "vue") {
        let page = match w {
            "ref" | "computed" | "reactive" | "readonly" | "watch" | "watchEffect"
            | "watchPostEffect" | "watchSyncEffect" => "reactivity-core.html",
            "isRef" | "unref" | "toRef" | "toValue" | "toRefs" | "isProxy" | "isReactive"
            | "isReadonly" => "reactivity-utilities.html",
            "shallowRef" | "triggerRef" | "customRef" | "shallowReactive" | "shallowReadonly"
            | "toRaw" | "markRaw" | "effectScope" => "reactivity-advanced.html",
            "onMounted" | "onUpdated" | "onUnmounted" | "onBeforeMount" | "onBeforeUpdate"
            | "onBeforeUnmount" | "onErrorCaptured" | "onActivated" | "onDeactivated" => {
                "composition-api-lifecycle.html"
            }
            "provide" | "inject" | "hasInjectionContext" => {
                "composition-api-dependency-injection.html"
            }
            "nextTick" | "defineComponent" | "defineAsyncComponent" | "version" => "general.html",
            "createApp" | "createSSRApp" => "application.html",
            "defineProps" | "defineEmits" | "defineExpose" | "defineModel" | "defineSlots"
            | "defineOptions" => "sfc-script-setup.html",
            _ => "",
        };
        if !page.is_empty() {
            return Some(format!(
                "https://vuejs.org/api/{page}#{}",
                w.to_ascii_lowercase()
            ));
        }
    }
    if sym.imports.iter().any(|i| i == "react") {
        let react = [
            "createContext",
            "forwardRef",
            "lazy",
            "memo",
            "startTransition",
            "use",
            "Fragment",
            "Profiler",
            "StrictMode",
            "Suspense",
        ];
        let hook = w.len() > 3 && w.starts_with("use") && w[3..].starts_with(char::is_uppercase);
        if hook || react.contains(&w) {
            return Some(format!("https://react.dev/reference/react/{w}"));
        }
        if w == "createRoot" || w == "hydrateRoot" {
            return Some(format!("https://react.dev/reference/react-dom/client/{w}"));
        }
    }
    None
}

fn html_url(tag: &str) -> Option<String> {
    // Components (`MyButton`, `router-link`) have no HTML page.
    if tag.contains('-') || tag.chars().any(|c| c.is_uppercase()) {
        return None;
    }
    let page = if tag.len() == 2 && tag.starts_with('h') && tag[1..].parse::<u8>().is_ok() {
        "Heading_Elements".to_string()
    } else {
        tag.to_string()
    };
    Some(format!("{MDN}HTML/Element/{page}"))
}

fn css_url(sym: &DocSymbol) -> Option<String> {
    let w = sym.word.as_str();
    if let Some(prefix) = &sym.css_prefix {
        return Some(format!("{MDN}CSS/{prefix}{w}"));
    }
    if sym.css_property && !w.starts_with("--") && !w.starts_with('$') {
        return Some(format!("{MDN}CSS/{w}"));
    }
    None
}

const LUA_LIBS: &[(&str, &str)] = &[
    ("coroutine", "6.2"),
    ("package", "6.3"),
    ("string", "6.4"),
    ("utf8", "6.5"),
    ("table", "6.6"),
    ("math", "6.7"),
    ("io", "6.8"),
    ("os", "6.9"),
    ("debug", "6.10"),
];

const LUA_BASE: &[&str] = &[
    "assert",
    "collectgarbage",
    "dofile",
    "error",
    "getmetatable",
    "ipairs",
    "load",
    "loadfile",
    "next",
    "pairs",
    "pcall",
    "print",
    "rawequal",
    "rawget",
    "rawlen",
    "rawset",
    "require",
    "select",
    "setmetatable",
    "tonumber",
    "tostring",
    "type",
    "warn",
    "xpcall",
    "_G",
    "_VERSION",
];

fn lua_url(sym: &DocSymbol) -> Option<String> {
    let w = sym.word.as_str();
    if let Some(q) = sym.qualifier.as_deref() {
        return LUA_LIBS
            .iter()
            .any(|(l, _)| *l == q)
            .then(|| format!("{LUA_MANUAL}#pdf-{q}.{w}"));
    }
    if LUA_BASE.contains(&w) {
        return Some(format!("{LUA_MANUAL}#pdf-{w}"));
    }
    LUA_LIBS
        .iter()
        .find(|(l, _)| *l == w)
        .map(|(_, section)| format!("{LUA_MANUAL}#{section}"))
}

// ---- Scanning code ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mark {
    Open,
    Close,
    /// `;` (a declaration without a body).
    End,
}

/// The braces and semicolons of brace languages outside strings and comments, with their
/// line.
fn brace_marks(text: &str, language: &str) -> Vec<(usize, Mark)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut line = 0usize;
    let mut i = 0;
    let hash_comments = language == "php";
    let line_comments = !matches!(language, "css");
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            '\n' => line += 1,
            '/' if next == Some('*') => {
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
                i += 1;
            }
            '/' if next == Some('/') && line_comments => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '#' if hash_comments && next != Some('[') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '"' | '\'' | '`' => {
                let quote = c;
                i += 1;
                while i < chars.len() && chars[i] != quote {
                    if chars[i] == '\\' {
                        i += 1;
                    } else if chars[i] == '\n' {
                        if quote != '`' && language != "php" {
                            break;
                        }
                        line += 1;
                    }
                    i += 1;
                }
                if chars.get(i) == Some(&'\n') {
                    line += 1;
                }
            }
            '{' => out.push((line, Mark::Open)),
            '}' => out.push((line, Mark::Close)),
            ';' => out.push((line, Mark::End)),
            _ => {}
        }
        i += 1;
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LuaMark {
    /// A block opened by `function`.
    Function,
    /// `do`, `then`, `repeat`.
    Block,
    /// `end`, `until`.
    Close,
}

/// Lua's block keywords outside strings and comments, with their line.
fn lua_marks(text: &str) -> Vec<(usize, LuaMark)> {
    let mut out = Vec::new();
    let mut in_block_comment = false;
    for (n, line) in lines_of(text).iter().enumerate() {
        let mut rest: &str = line;
        if in_block_comment {
            match rest.find("]]") {
                Some(i) => {
                    rest = &rest[i + 2..];
                    in_block_comment = false;
                }
                None => continue,
            }
        }
        let mut code = String::new();
        let chars: Vec<char> = rest.chars().collect();
        let mut i = 0;
        let mut skip_then = false;
        while i < chars.len() {
            let c = chars[i];
            if c == '-' && chars.get(i + 1) == Some(&'-') {
                let tail: String = chars[i + 2..].iter().collect();
                if tail.starts_with("[[") && !tail.contains("]]") {
                    in_block_comment = true;
                }
                break;
            }
            if c == '"' || c == '\'' {
                i += 1;
                while i < chars.len() && chars[i] != c {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    i += 1;
                }
                code.push(' ');
            } else {
                code.push(c);
            }
            i += 1;
        }
        for word in code.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
            match word {
                "function" => out.push((n, LuaMark::Function)),
                "elseif" => skip_then = true,
                "then" if skip_then => skip_then = false,
                "do" | "then" | "repeat" => out.push((n, LuaMark::Block)),
                "end" | "until" => out.push((n, LuaMark::Close)),
                _ => {}
            }
        }
    }
    out
}

// ---- Quick Definition ----

/// The lines Quick Definition shows for a declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetRange {
    pub start_line: u32,
    /// Inclusive.
    pub end_line: u32,
    /// The declaration is longer than [`MAX_SNIPPET_LINES`].
    pub truncated: bool,
}

/// The longest Quick Definition snippet.
pub const MAX_SNIPPET_LINES: u32 = 80;

/// How far below the declaration line its body may start (multi-line signatures).
const BODY_SEARCH_LINES: usize = 12;

fn is_comment_line(trimmed: &str, language: &str) -> bool {
    if language == "lua" {
        return trimmed.starts_with("--");
    }
    trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || (language == "php" && trimmed.starts_with('#') && !trimmed.starts_with("#["))
}

fn is_attribute_line(trimmed: &str, language: &str) -> bool {
    (language == "php" && trimmed.starts_with("#["))
        || (is_script(language) && trimmed.starts_with('@'))
}

/// The declaration at `line` with the docblock, comments and attributes above it and its
/// body below it.
pub fn definition_snippet(text: &str, line: u32, language: &str) -> SnippetRange {
    let lines = lines_of(text);
    let last = lines.len().saturating_sub(1);
    let line = (line as usize).min(last);

    let mut start = line;
    while start > 0 {
        let prev = lines[start - 1].trim();
        if prev.is_empty() {
            break;
        }
        if is_comment_line(prev, language)
            || is_attribute_line(prev, language)
            || prev.ends_with("*/")
        {
            start -= 1;
        } else {
            break;
        }
    }

    let end = if language == "lua" {
        lua_end(text, line)
    } else if has_braces(language) {
        brace_end(text, line, language)
    } else {
        line
    };
    let end = end.max(line).min(last);
    let truncated = end + 1 - start > MAX_SNIPPET_LINES as usize;
    let end = if truncated {
        start + MAX_SNIPPET_LINES as usize - 1
    } else {
        end
    };
    SnippetRange {
        start_line: start as u32,
        end_line: end as u32,
        truncated,
    }
}

/// The line of the `}` closing the body that starts at or below `line`, or of the `;` that
/// ends a declaration without one.
fn brace_end(text: &str, line: usize, language: &str) -> usize {
    let mut depth = 0usize;
    for (l, mark) in brace_marks(text, language) {
        if l < line {
            continue;
        }
        if depth == 0 && l > line + BODY_SEARCH_LINES {
            return line;
        }
        match mark {
            Mark::End if depth == 0 => return l,
            Mark::Open => depth += 1,
            Mark::Close if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    return l;
                }
            }
            Mark::Close => return line,
            _ => {}
        }
    }
    line
}

/// The line of the `end` closing the block opened at `line`.
fn lua_end(text: &str, line: usize) -> usize {
    let mut depth = 0usize;
    let mut opened = false;
    for (l, mark) in lua_marks(text) {
        if l < line {
            continue;
        }
        match mark {
            LuaMark::Function | LuaMark::Block => {
                depth += 1;
                opened = true;
            }
            LuaMark::Close if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    return l;
                }
            }
            LuaMark::Close => return line,
        }
        if !opened && l > line {
            return line;
        }
    }
    line
}

// ---- Context Info ----

/// One header line of Context Info.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextLine {
    /// 0-based line in the file.
    pub line: u32,
    /// The line without the indentation the headers share.
    pub text: String,
}

const CONTROL_WORDS: &[&str] = &[
    "if", "else", "elseif", "for", "foreach", "while", "do", "switch", "try", "catch", "finally",
    "match", "return", "case", "default", "with", "echo", "throw", "new",
];

const DECLARATION_WORDS: &[&str] = &[
    "class",
    "interface",
    "trait",
    "enum",
    "function",
    "namespace",
    "fn",
    "module",
    "declare",
    "impl",
    "struct",
    "object",
];

fn words(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .filter(|w| !w.is_empty())
}

/// Whether a block header declares something (a class, function or method), not a control
/// statement or an object literal.
fn is_declaration_header(header: &str, language: &str) -> bool {
    if is_css(language) {
        return true;
    }
    let trimmed = header.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('}') {
        return false;
    }
    if words(trimmed).any(|w| DECLARATION_WORDS.contains(&w)) {
        return true;
    }
    let first = words(trimmed).next().unwrap_or("");
    if CONTROL_WORDS.contains(&first) {
        return false;
    }
    if trimmed.contains("=>") {
        return true;
    }
    // JS/TS methods: `async save(order) {`, `get total(): number {`.
    is_script(language)
        && trimmed.contains('(')
        && !trimmed.contains('=')
        && trimmed
            .split('(')
            .next()
            .is_some_and(|head| words(head).count() >= 1 && !head.trim_end().ends_with('.'))
}

/// The first line of the header whose block opens on `open_line`: the line above a lone `{`
/// and the start of a multi-line parameter list.
pub(crate) fn header_start(lines: &[&str], open_line: usize) -> usize {
    let mut l = open_line;
    if lines[l].trim().starts_with('{') {
        while l > 0 {
            l -= 1;
            if !lines[l].trim().is_empty() {
                break;
            }
        }
    }
    let mut balance: i32 = 0;
    let mut cur = l;
    loop {
        for c in lines[cur].chars() {
            match c {
                '(' => balance -= 1,
                ')' => balance += 1,
                _ => {}
            }
        }
        if balance <= 0 || cur == 0 || l - cur >= BODY_SEARCH_LINES {
            return cur;
        }
        cur -= 1;
    }
}

/// The headers of the declarations that enclose `line`, outermost first.
pub fn context_headers(text: &str, line: u32, language: &str) -> Vec<ContextLine> {
    let lines = lines_of(text);
    let line = line as usize;
    let mut open: Vec<usize> = Vec::new();
    if language == "lua" {
        let mut stack: Vec<(usize, bool)> = Vec::new();
        for (l, mark) in lua_marks(text) {
            if l >= line {
                break;
            }
            match mark {
                LuaMark::Function => stack.push((l, true)),
                LuaMark::Block => stack.push((l, false)),
                LuaMark::Close => {
                    stack.pop();
                }
            }
        }
        open = stack
            .into_iter()
            .filter(|(_, f)| *f)
            .map(|(l, _)| l)
            .collect();
    } else if has_braces(language) {
        for (l, mark) in brace_marks(text, language) {
            if l >= line {
                break;
            }
            match mark {
                Mark::Open => open.push(l),
                Mark::Close => {
                    open.pop();
                }
                Mark::End => {}
            }
        }
    }
    let mut headers: Vec<(usize, String)> = Vec::new();
    for l in open {
        let start = if language == "lua" {
            l
        } else {
            header_start(&lines, l)
        };
        let text = lines[start].trim_end();
        if language != "lua" && !is_declaration_header(text, language) {
            continue;
        }
        if headers.last().is_some_and(|(s, _)| *s == start) {
            continue;
        }
        headers.push((start, text.to_string()));
    }
    let indent = headers
        .iter()
        .map(|(_, t)| t.len() - t.trim_start().len())
        .min()
        .unwrap_or(0);
    headers
        .into_iter()
        .map(|(l, t)| ContextLine {
            line: l as u32,
            text: t.get(indent..).unwrap_or(t.trim_start()).to_string(),
        })
        .collect()
}

// ---- Type Info ----

/// Splits `s` at the first `sep` outside brackets, quotes and generics.
fn split_top(s: &str, sep: &str) -> Option<(usize, usize)> {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '>' if !s[..i].ends_with('=') && !s[..i].ends_with('-') => depth -= 1,
            _ => {}
        }
        if depth == 0 && s[i..].starts_with(sep) {
            return Some((i, i + sep.len()));
        }
    }
    None
}

/// The index after the `)` matching the `(` at `open`.
fn close_paren(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in s[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

const MODIFIERS: &[&str] = &[
    "public",
    "protected",
    "private",
    "static",
    "readonly",
    "abstract",
    "final",
    "var",
    "const",
    "let",
    "local",
    "declare",
    "export",
    "default",
    "async",
    "override",
];

fn non_empty(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// The type of a declaration from its signature: a function's return type, a property's,
/// variable's or parameter's type, a class's name.
///
/// `public function orders(): HasMany` → `HasMany`, `public ?User $owner` → `?User`,
/// `(parameter) total: number` → `number`, `local count: integer = 0` → `integer`.
pub fn type_from_signature(signature: &str) -> Option<String> {
    let mut s = signature.trim();
    for suffix in ["{ }", "{}", "{", ";"] {
        s = s.strip_suffix(suffix).unwrap_or(s).trim_end();
    }
    // TypeScript's `(parameter) `, `(property) `, `(method) ` prefixes.
    if s.starts_with('(') {
        if let Some(end) = s.find(") ") {
            if !s[1..end].contains(['(', ',', ':']) {
                s = s[end + 2..].trim_start();
            }
        }
    }
    let mut rest = s;
    loop {
        let word = rest.split_whitespace().next().unwrap_or("");
        if MODIFIERS.contains(&word) {
            rest = rest[word.len()..].trim_start();
        } else {
            break;
        }
    }
    let first = rest.split_whitespace().next().unwrap_or("");
    if matches!(first, "class" | "interface" | "trait" | "enum" | "type") {
        let name = rest[first.len()..].trim_start();
        let end = name
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '\\'))
            .unwrap_or(name.len());
        return non_empty(&name[..end]);
    }
    // Drop a default value.
    let rest = match split_top(rest, " = ") {
        Some((i, _)) => &rest[..i],
        None => rest,
    };
    // Functions and methods: the return type after the parameter list.
    let paren = rest.find('(');
    let colon = split_top(rest, ":").map(|(i, _)| i);
    if let Some(open) = paren.filter(|p| colon.is_none_or(|c| *p < c)) {
        let after = &rest[close_paren(rest, open)?..];
        let after = after.trim_start();
        let ret = after
            .strip_prefix(':')
            .or_else(|| after.strip_prefix("->"))
            .or_else(|| after.strip_prefix("=>"))?;
        return non_empty(ret);
    }
    if let Some((_, after)) = split_top(rest, ":") {
        return non_empty(&rest[after..]);
    }
    // PHP: `?User $owner`, `\Illuminate\Support\Collection<int, Order> $orders`.
    if let Some(dollar) = rest.rfind(" $") {
        return non_empty(&rest[..dollar]);
    }
    None
}

/// The type shown by Type Info for a language server hover: from the signature in its
/// first code block, else from an `@var` tag. Class declarations answer their qualified
/// name (`\App\Models\User`) when the hover's title has it.
pub fn type_from_hover(markdown: &str) -> Option<String> {
    let mut code: Vec<&str> = Vec::new();
    let mut in_code = false;
    let mut title = None;
    for line in markdown.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            if in_code {
                break;
            }
            in_code = true;
            continue;
        }
        if in_code {
            if !t.is_empty() && t != "<?php" && !t.starts_with("#[") && !t.starts_with('@') {
                code.push(t);
            }
        } else if title.is_none() && t.starts_with("__") && t.ends_with("__") && t.len() > 4 {
            title = Some(t[2..t.len() - 2].to_string());
        }
    }
    if !code.is_empty() {
        let signature = code.join(" ");
        if let Some(ty) = type_from_signature(&signature) {
            let class_like =
                words(&signature).any(|w| matches!(w, "class" | "interface" | "trait" | "enum"));
            if let (true, Some(fqn)) = (class_like, &title) {
                if fqn.ends_with(&ty) && !fqn.contains("::") {
                    return Some(format!("\\{}", fqn.trim_start_matches('\\')));
                }
            }
            return Some(ty);
        }
    }
    for line in markdown.lines() {
        if let Some(rest) = line.trim().strip_prefix("_@var_") {
            let ty = rest.trim().trim_matches('`');
            let ty = ty.split('`').next().unwrap_or(ty);
            return non_empty(ty);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(text: &str, line: u32, character: u32, language: &str) -> DocSymbol {
        symbol_at(text, Position::new(line, character), language).expect("symbol")
    }

    #[test]
    fn languages_by_extension() {
        assert_eq!(language_of("app/Models/User.php"), "php");
        assert_eq!(language_of("resources/views/a.blade.php"), "blade");
        assert_eq!(language_of("src/App.TSX"), "typescriptreact");
        assert_eq!(language_of("init.lua"), "lua");
        assert_eq!(language_of("README"), "plaintext");
    }

    #[test]
    fn reads_the_symbol_at_the_caret() {
        let s = sym("const x = JSON.parse(raw);", 0, 16, "javascript");
        assert_eq!(s.word, "parse");
        assert_eq!(s.qualifier.as_deref(), Some("JSON"));
        assert!(s.call);

        let s = sym("$user->save();", 0, 0, "php");
        assert_eq!(s.word, "user");
        let s = sym("$user->save();", 0, 9, "php");
        assert_eq!(
            (s.word.as_str(), s.qualifier.as_deref()),
            ("save", Some("$user"))
        );

        let s = sym("  <section class=\"a\">", 0, 4, "html");
        assert!(s.tag);
        assert_eq!(s.word, "section");

        let s = sym(".a {\n  font-size: 12px;\n}", 1, 4, "css");
        assert!(s.css_property);
        assert_eq!(s.word, "font-size");

        let s = sym("import { ref } from 'vue'", 0, 10, "typescript");
        assert_eq!(s.imports, vec!["vue".to_string()]);

        assert!(symbol_at("a  b", Position::new(0, 2), "php").is_none());
        assert!(symbol_at("x = 42", Position::new(0, 5), "javascript").is_none());
    }

    #[test]
    fn php_builtins_open_php_net() {
        let mut s = sym("strlen($a);", 0, 2, "php");
        s.declaration = Some(DocTarget {
            path: "library://php-stubs/standard/standard_1.php".into(),
            name: "strlen()".into(),
            container: Some("standard".into()),
        });
        assert_eq!(
            external_doc_url(&s).as_deref(),
            Some("https://www.php.net/manual/en/function.strlen.php")
        );

        s.declaration.as_mut().unwrap().name = "str_contains()".into();
        assert!(external_doc_url(&s)
            .unwrap()
            .ends_with("function.str-contains.php"));

        s.declaration.as_mut().unwrap().name = "DateTime::format()".into();
        assert!(external_doc_url(&s)
            .unwrap()
            .ends_with("/datetime.format.php"));

        s.declaration.as_mut().unwrap().name = "DateTime::__construct()".into();
        assert!(external_doc_url(&s)
            .unwrap()
            .ends_with("/datetime.construct.php"));

        s.declaration.as_mut().unwrap().name = "Throwable".into();
        assert!(external_doc_url(&s)
            .unwrap()
            .ends_with("/class.throwable.php"));

        s.declaration.as_mut().unwrap().name = "Random\\Randomizer".into();
        assert!(external_doc_url(&s)
            .unwrap()
            .ends_with("/class.random-randomizer.php"));
    }

    #[test]
    fn php_keywords_project_symbols_and_laravel() {
        let s = sym("foreach ($a as $b) {}", 0, 3, "php");
        assert!(external_doc_url(&s)
            .unwrap()
            .ends_with("control-structures.foreach.php"));

        let mut s = sym("$user->orders();", 0, 9, "php");
        s.declaration = Some(DocTarget {
            path: "app/Models/User.php".into(),
            name: "User::orders()".into(),
            container: Some("App\\Models".into()),
        });
        assert_eq!(external_doc_url(&s), None);

        let mut s = sym("$users->pluck('id');", 0, 10, "php");
        s.declaration = Some(DocTarget {
            path: "vendor/laravel/framework/src/Illuminate/Support/Collection.php".into(),
            ..Default::default()
        });
        assert_eq!(
            external_doc_url(&s).as_deref(),
            Some("https://laravel.com/api/master/Illuminate/Support/Collection.html#method_pluck")
        );
    }

    #[test]
    fn web_languages_open_mdn_and_framework_references() {
        let url = |text: &str, ch: u32, lang: &str| external_doc_url(&sym(text, 0, ch, lang));
        assert_eq!(
            url("JSON.parse(raw)", 6, "javascript").as_deref(),
            Some("https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/JSON/parse")
        );
        assert!(url("new Promise(r)", 6, "typescript")
            .unwrap()
            .ends_with("Global_Objects/Promise"));
        assert!(url("document.querySelector('a')", 10, "javascript")
            .unwrap()
            .ends_with("API/Document/querySelector"));
        assert!(url("console.log(1)", 9, "javascript")
            .unwrap()
            .ends_with("API/console/log_static"));
        assert!(url("await fetch(u)", 7, "javascript")
            .unwrap()
            .ends_with("API/Window/fetch"));
        assert!(url("for (const a of b) {}", 1, "javascript")
            .unwrap()
            .ends_with("Statements/for"));
        assert_eq!(url("order.total()", 7, "javascript"), None);

        assert_eq!(
            url(
                "import { ref } from 'vue'; const a = ref(1)",
                38,
                "typescript"
            )
            .as_deref(),
            Some("https://vuejs.org/api/reactivity-core.html#ref")
        );
        assert_eq!(
            url(
                "import { useState } from 'react'; useState(0)",
                36,
                "typescriptreact"
            )
            .as_deref(),
            Some("https://react.dev/reference/react/useState")
        );
        assert_eq!(url("const a = ref(1)", 11, "typescript"), None);

        assert!(url("<ul><li>", 5, "html")
            .unwrap()
            .ends_with("HTML/Element/li"));
        assert!(url("<h2>", 1, "vue")
            .unwrap()
            .ends_with("HTML/Element/Heading_Elements"));
        assert_eq!(url("<RouterLink to=\"/\">", 2, "vue"), None);

        assert!(url("  display: flex;", 4, "css")
            .unwrap()
            .ends_with("Web/CSS/display"));
        assert!(url("@media (max-width: 1px) {", 2, "css")
            .unwrap()
            .ends_with("Web/CSS/@media"));
        assert!(url("a:hover {", 3, "css")
            .unwrap()
            .ends_with("Web/CSS/:hover"));
        assert_eq!(url("  color: red;", 10, "css"), None);
    }

    #[test]
    fn lua_opens_the_reference_manual() {
        let url = |text: &str, ch: u32| external_doc_url(&sym(text, 0, ch, "lua"));
        assert_eq!(
            url("local s = string.format('%d', 1)", 18).as_deref(),
            Some("https://www.lua.org/manual/5.4/manual.html#pdf-string.format")
        );
        assert!(url("print(1)", 1).unwrap().ends_with("#pdf-print"));
        assert!(url("table.insert(t, 1)", 2).unwrap().ends_with("#6.6"));
        assert_eq!(url("player:move()", 9), None);
    }

    const USER: &str = r#"<?php

namespace App\Models;

/**
 * A customer.
 */
#[ObservedBy(UserObserver::class)]
class User extends Model
{
    protected $fillable = ['name'];

    /** The user's orders. */
    public function orders(): HasMany
    {
        return $this->hasMany(Order::class); // {
    }

    abstract public function total(): float;

    public function label(
        string $prefix,
    ): string {
        $s = "}";
        return $prefix . $this->name;
    }
}
"#;

    #[test]
    fn php_definition_snippets_include_docs_and_body() {
        let class = definition_snippet(USER, 8, "php");
        assert_eq!((class.start_line, class.end_line), (4, 26));
        assert!(!class.truncated);

        let method = definition_snippet(USER, 13, "php");
        assert_eq!((method.start_line, method.end_line), (12, 16));

        let property = definition_snippet(USER, 10, "php");
        assert_eq!((property.start_line, property.end_line), (10, 10));

        let abstract_method = definition_snippet(USER, 18, "php");
        assert_eq!(
            (abstract_method.start_line, abstract_method.end_line),
            (18, 18)
        );

        let multi = definition_snippet(USER, 20, "php");
        assert_eq!((multi.start_line, multi.end_line), (20, 25));
    }

    #[test]
    fn long_definitions_are_truncated() {
        let body: String = (0..200).map(|i| format!("  let a{i} = {i};\n")).collect();
        let text = format!("function big() {{\n{body}}}\n");
        let r = definition_snippet(&text, 0, "javascript");
        assert!(r.truncated);
        assert_eq!(r.end_line - r.start_line + 1, MAX_SNIPPET_LINES);
    }

    #[test]
    fn lua_definition_snippets_end_at_end() {
        let text = "-- Moves the player.\nfunction Player:move(dx)\n  if dx > 0 then\n    self.x = self.x + dx\n  end\nend\n\nreturn Player\n";
        let r = definition_snippet(text, 1, "lua");
        assert_eq!((r.start_line, r.end_line), (0, 5));
    }

    #[test]
    fn context_info_lists_enclosing_declarations() {
        let headers = context_headers(USER, 15, "php");
        let texts: Vec<_> = headers.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "class User extends Model",
                "    public function orders(): HasMany"
            ]
        );
        assert_eq!(headers[0].line, 8);

        let multi = context_headers(USER, 23, "php");
        assert_eq!(multi[1].text, "    public function label(");

        assert!(context_headers(USER, 2, "php").is_empty());

        let js = "export class Cart {\n  async total(items) {\n    if (items) {\n      for (const i of items) {\n        x();\n      }\n    }\n  }\n}\n";
        let texts: Vec<_> = context_headers(js, 4, "javascript")
            .into_iter()
            .map(|h| h.text)
            .collect();
        assert_eq!(texts, vec!["export class Cart {", "  async total(items) {"]);

        let css = "@media (max-width: 600px) {\n  .card {\n    color: red;\n  }\n}\n";
        let texts: Vec<_> = context_headers(css, 2, "css")
            .into_iter()
            .map(|h| h.text)
            .collect();
        assert_eq!(texts, vec!["@media (max-width: 600px) {", "  .card {"]);

        let lua = "local M = {}\nfunction M.run(xs)\n  for _, x in ipairs(xs) do\n    if x then\n      print(x)\n    end\n  end\nend\n";
        let texts: Vec<_> = context_headers(lua, 4, "lua")
            .into_iter()
            .map(|h| h.text)
            .collect();
        assert_eq!(texts, vec!["function M.run(xs)"]);
    }

    #[test]
    fn types_from_signatures() {
        let t = |s: &str| type_from_signature(s);
        assert_eq!(
            t("public function orders(): HasMany { }").as_deref(),
            Some("HasMany")
        );
        assert_eq!(
            t("public function hasMany(string $related, string|null $foreignKey = null): HasMany")
                .as_deref(),
            Some("HasMany")
        );
        assert_eq!(t("function boot()"), None);
        assert_eq!(t("public ?User $owner = null;").as_deref(), Some("?User"));
        assert_eq!(
            t("public readonly \\Illuminate\\Support\\Collection<int, Order> $orders;").as_deref(),
            Some("\\Illuminate\\Support\\Collection<int, Order>")
        );
        assert_eq!(t("$count"), None);
        assert_eq!(t("(parameter) total: number").as_deref(), Some("number"));
        assert_eq!(
            t("const items: Map<string, Item[]>").as_deref(),
            Some("Map<string, Item[]>")
        );
        assert_eq!(
            t("(method) Array<number>.map<U>(cb: (v: number) => U): U[]").as_deref(),
            Some("U[]")
        );
        assert_eq!(t("local count: integer = 0").as_deref(), Some("integer"));
        assert_eq!(
            t("function M.run(xs: table) -> boolean").as_deref(),
            Some("boolean")
        );
        assert_eq!(t("final class User extends Model").as_deref(), Some("User"));
    }

    #[test]
    fn types_from_hovers() {
        let hover = "__App\\Models\\User::totalSpent__\n\n```php\n<?php\npublic function totalSpent(): float { }\n```\n\n_@return_ `float`";
        assert_eq!(type_from_hover(hover).as_deref(), Some("float"));

        let class = "__App\\Models\\User__\n\n```php\n<?php\n#[ObservedBy(X::class)]\nclass User extends Model { }\n```";
        assert_eq!(
            type_from_hover(class).as_deref(),
            Some("\\App\\Models\\User")
        );

        let var = "```php\n<?php\n$total\n```\n\n_@var_ `int|float`";
        assert_eq!(type_from_hover(var).as_deref(), Some("int|float"));

        let ts = "```typescript\nlet count: number\n```";
        assert_eq!(type_from_hover(ts).as_deref(), Some("number"));

        assert_eq!(type_from_hover("Just text."), None);
    }
}
