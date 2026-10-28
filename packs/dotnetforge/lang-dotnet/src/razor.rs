//! Blazor components for the editor: every `.razor` file is a component named after the
//! file, with its `[Parameter]` properties (from `@code` blocks and the `.razor.cs`
//! code-behind), `@page` routes, `@typeparam`s and `@namespace`. Go to Declaration on
//! `<Counter>` opens `Counter.razor`; completion inside the tag offers its parameters.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::csharp::{attribute_list, tokenize, Token, MODIFIERS};
use crate::paths::{file_name, parent};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentParameter {
    pub name: String,
    pub type_name: String,
    /// `[EditorRequired]`.
    #[serde(default)]
    pub required: bool,
    /// `[CascadingParameter]` (not set as an attribute; listed for hover only).
    #[serde(default)]
    pub cascading: bool,
    /// `[Parameter(CaptureUnmatchedValues = true)]`.
    #[serde(default)]
    pub captures_unmatched: bool,
    pub file: String,
    /// 0-based line.
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RazorComponent {
    /// `Counter` for `Counter.razor`.
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_params: Vec<String>,
    pub parameters: Vec<ComponentParameter>,
    /// The project file (filled in by the caller).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

/// The component name of a `.razor` path (`Pages/Counter.razor` → `Counter`).
pub fn component_name(path: &str) -> Option<String> {
    let name = file_name(path);
    let stem = name.strip_suffix(".razor")?;
    (!stem.is_empty() && !stem.starts_with('_')).then(|| stem.to_string())
}

/// `[Parameter]` properties declared in C# code (a `@code { … }` block or a code-behind).
pub fn parameters_in_code(file: &str, code: &str, line_offset: u32) -> Vec<ComponentParameter> {
    let tokens = tokenize(code);
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if !tokens[i].is('[') {
            i += 1;
            continue;
        }
        let mut attrs = Vec::new();
        let mut j = i;
        while j < tokens.len() && tokens[j].is('[') {
            let (a, end) = attribute_list(&tokens, j);
            attrs.extend(a);
            j = end;
        }
        let param = attrs.iter().find(|a| a.name == "Parameter");
        let cascading = attrs.iter().any(|a| a.name == "CascadingParameter");
        if param.is_none() && !cascading {
            i = j;
            continue;
        }
        // `public string Title { get; set; }` / `= default!;`
        let mut decl: Vec<&Token> = Vec::new();
        let mut k = j;
        while k < tokens.len() && !tokens[k].is('{') && !tokens[k].is(';') && !tokens[k].is('=') {
            decl.push(&tokens[k]);
            k += 1;
        }
        let decl: Vec<&Token> = decl
            .into_iter()
            .skip_while(|t| t.ident().is_some_and(|x| MODIFIERS.contains(&x)))
            .collect();
        if let Some((name_tok, ty)) = decl.split_last() {
            if let Some(name) = name_tok.ident() {
                let ty: Vec<Token> = ty.iter().map(|t| (*t).clone()).collect();
                out.push(ComponentParameter {
                    name: name.to_string(),
                    type_name: crate::csharp::text(&ty),
                    required: attrs.iter().any(|a| a.name == "EditorRequired"),
                    cascading,
                    captures_unmatched: param.is_some_and(|p| {
                        p.named
                            .iter()
                            .any(|(k, v)| k == "CaptureUnmatchedValues" && v == "true")
                    }),
                    file: file.to_string(),
                    line: name_tok.line + line_offset,
                });
            }
        }
        i = k;
    }
    out
}

/// The `@code` / `@functions` blocks of a Razor file with their first line.
pub fn code_blocks(text: &str) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut line = 0u32;
    while i < bytes.len() {
        if bytes[i] == '\n' {
            line += 1;
        }
        if bytes[i] == '@' {
            let rest: String = bytes[i + 1..bytes.len().min(i + 12)].iter().collect();
            let kw = if rest.starts_with("code") {
                4
            } else if rest.starts_with("functions") {
                9
            } else {
                0
            };
            if kw > 0 {
                let mut j = i + 1 + kw;
                while j < bytes.len() && bytes[j].is_whitespace() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == '{' {
                    let start_line =
                        line + bytes[i..j].iter().filter(|c| **c == '\n').count() as u32;
                    let mut depth = 0;
                    let mut k = j;
                    while k < bytes.len() {
                        match bytes[k] {
                            '{' => depth += 1,
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        k += 1;
                    }
                    let code: String = bytes[j + 1..k.min(bytes.len())].iter().collect();
                    line = start_line + code.matches('\n').count() as u32;
                    out.push((start_line, code));
                    i = k + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

fn directive<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let rest = line.trim().strip_prefix('@')?.strip_prefix(name)?;
    rest.starts_with(char::is_whitespace).then(|| rest.trim())
}

/// A component from its `.razor` file.
pub fn scan_razor(path: &str, text: &str) -> Option<RazorComponent> {
    let name = component_name(path)?;
    let mut c = RazorComponent {
        name,
        path: path.to_string(),
        namespace: None,
        routes: Vec::new(),
        type_params: Vec::new(),
        parameters: Vec::new(),
        project: None,
    };
    for line in text.lines() {
        if let Some(r) = directive(line, "page") {
            c.routes.push(r.trim_matches('"').to_string());
        } else if let Some(ns) = directive(line, "namespace") {
            c.namespace = Some(ns.to_string());
        } else if let Some(tp) = directive(line, "typeparam") {
            if let Some(n) = tp.split_whitespace().next() {
                c.type_params.push(n.to_string());
            }
        }
    }
    for (line, code) in code_blocks(text) {
        c.parameters.extend(parameters_in_code(path, &code, line));
    }
    Some(c)
}

/// All components of a project: `.razor` files plus parameters from their code-behind
/// (`Counter.razor.cs`) and `@namespace` from the nearest `_Imports.razor`.
pub fn components(files: &BTreeMap<String, String>) -> Vec<RazorComponent> {
    let mut out: Vec<RazorComponent> = files.iter().filter_map(|(p, t)| scan_razor(p, t)).collect();
    for c in &mut out {
        if let Some(code) = files.get(&format!("{}.cs", c.path)) {
            let behind = format!("{}.cs", c.path);
            for p in parameters_in_code(&behind, code, 0) {
                if !c.parameters.iter().any(|x| x.name == p.name) {
                    c.parameters.push(p);
                }
            }
        }
        if c.namespace.is_none() {
            let mut dir = parent(&c.path);
            loop {
                let imports = if dir.is_empty() {
                    "_Imports.razor".to_string()
                } else {
                    format!("{dir}/_Imports.razor")
                };
                if let Some(ns) = files
                    .get(&imports)
                    .and_then(|t| t.lines().find_map(|l| directive(l, "namespace")))
                {
                    c.namespace = Some(ns.to_string());
                    break;
                }
                if dir.is_empty() {
                    break;
                }
                dir = parent(&dir);
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));
    out
}

/// The component tag at a position in a Razor file: `<Counter` / `</Counter` / `<Forms.Input`
/// with the cursor on the name (only capitalized names are components).
pub fn tag_at(line: &str, column: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let col = column.min(chars.len());
    let mut start = col;
    while start > 0
        && (chars[start - 1].is_alphanumeric()
            || chars[start - 1] == '.'
            || chars[start - 1] == '_')
    {
        start -= 1;
    }
    let mut end = col;
    while end < chars.len()
        && (chars[end].is_alphanumeric() || chars[end] == '.' || chars[end] == '_')
    {
        end += 1;
    }
    if start == 0 || start == end {
        return None;
    }
    let before = chars[start - 1];
    let opener = before == '<' || before == '/' && start >= 2 && chars[start - 2] == '<';
    let word: String = chars[start..end].iter().collect();
    let name = word.rsplit('.').next().unwrap_or(&word).to_string();
    (opener && name.chars().next().is_some_and(char::is_uppercase)).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATING: &str = r#"@namespace Bookstore.Web.Shared
@typeparam TItem

<div class="rating">@Value / @Max</div>

@code {
    /// The rating.
    [Parameter, EditorRequired]
    public int Value { get; set; }

    [Parameter] public int Max { get; set; } = 5;

    [Parameter(CaptureUnmatchedValues = true)]
    public Dictionary<string, object>? Attributes { get; set; }

    [CascadingParameter] private Task<AuthenticationState> Auth { get; set; } = default!;

    private int hidden;
}
"#;

    #[test]
    fn scans_components() {
        let c = scan_razor("src/Web/Shared/Rating.razor", RATING).unwrap();
        assert_eq!(c.name, "Rating");
        assert_eq!(c.namespace.as_deref(), Some("Bookstore.Web.Shared"));
        assert_eq!(c.type_params, ["TItem"]);
        let p: Vec<(&str, &str, bool, u32)> = c
            .parameters
            .iter()
            .map(|p| (p.name.as_str(), p.type_name.as_str(), p.required, p.line))
            .collect();
        assert_eq!(
            p,
            [
                ("Value", "int", true, 8),
                ("Max", "int", false, 10),
                ("Attributes", "Dictionary<string, object>?", false, 13),
                ("Auth", "Task<AuthenticationState>", false, 15),
            ]
        );
        assert!(c.parameters[2].captures_unmatched);
        assert!(c.parameters[3].cascading);
        assert!(scan_razor("src/Web/_Imports.razor", "").is_none());
    }

    #[test]
    fn code_behind_and_imports() {
        let mut files = BTreeMap::new();
        files.insert(
            "src/Web/_Imports.razor".to_string(),
            "@using X\n@namespace Bookstore.Web\n".to_string(),
        );
        files.insert(
            "src/Web/Pages/Counter.razor".to_string(),
            "@page \"/counter\"\n<button>+</button>".to_string(),
        );
        files.insert(
            "src/Web/Pages/Counter.razor.cs".to_string(),
            "public partial class Counter : ComponentBase\n{\n    [Parameter]\n    public int Step { get; set; } = 1;\n}\n".to_string(),
        );
        let c = components(&files);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].routes, ["/counter"]);
        assert_eq!(c[0].namespace.as_deref(), Some("Bookstore.Web"));
        assert_eq!(c[0].parameters[0].name, "Step");
        assert_eq!(c[0].parameters[0].file, "src/Web/Pages/Counter.razor.cs");
        assert_eq!(c[0].parameters[0].line, 3);
    }

    #[test]
    fn tags() {
        assert_eq!(
            tag_at("    <Rating Value=\"3\" />", 7).as_deref(),
            Some("Rating")
        );
        assert_eq!(tag_at("</Rating>", 4).as_deref(), Some("Rating"));
        assert_eq!(tag_at("<Shared.Rating />", 10).as_deref(), Some("Rating"));
        assert_eq!(tag_at("<div class=\"x\">", 2), None);
        assert_eq!(tag_at("Rating", 2), None);
    }
}
