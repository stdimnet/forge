//! Extract Component for React: the selected JSX elements become a function component in
//! a new file with a prop for every name they take from the enclosing component (`{total}`,
//! `onClick={() => select(item)}`, the array of a `.map()` outside the selection); names
//! bound inside (arrow function parameters) are not props, and components and helpers the
//! parent imports are imported by the new file. The parent renders
//! `<NewComponent total={total} />` and imports it. TypeScript files get a props interface.

use webforge_lang_web::extract::{
    after_imports, check_component_name, default_import, import_line, import_style, indent_unit,
    line_indent, rebase_spec, reindent, relative_spec, trimmed, ExtractComponentInfo,
    ExtractedComponent, ImportStyle, Replacement,
};
use webforge_lang_web::imports::{self, Import, Imported};
use webforge_lang_web::scope::{scan_jsx_elements, Names};

const STYLE: ImportStyle = ImportStyle {
    quote: '"',
    semicolon: true,
};

fn extension(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or("jsx")
}

struct Analysis {
    start: usize,
    end: usize,
    roots: usize,
    props: Vec<String>,
    imports: Vec<Import>,
}

fn analyse(path: &str, content: &str, start: usize, end: usize) -> Result<Analysis, String> {
    if !crate::is_script(path) {
        return Err("Extract Component works in JavaScript and TypeScript files".into());
    }
    let (start, end) = trimmed(content, start.min(end), start.max(end));
    if start >= end || content.as_bytes()[start] != b'<' {
        return Err("Select JSX elements to extract".into());
    }
    let mut names = Names::default();
    let roots = scan_jsx_elements(content, start, end, &mut names)
        .ok_or("The selection should be whole JSX elements")?;
    let parent_imports = imports::imports(content);
    let mut used = Vec::new();
    let mut props = Vec::new();
    for name in names.free() {
        match parent_imports.iter().find(|i| i.local == name) {
            Some(i) => used.push(i.clone()),
            None => props.push(name),
        }
    }
    Ok(Analysis {
        start,
        end,
        roots,
        props,
        imports: used,
    })
}

/// Extract Component's data for the selection `start..end`.
pub fn prepare(
    path: &str,
    content: &str,
    start: usize,
    end: usize,
) -> Result<ExtractComponentInfo, String> {
    let a = analyse(path, content, start, end)?;
    let ext = match extension(path) {
        "ts" => "tsx",
        "js" | "mjs" => "jsx",
        e => e,
    };
    Ok(ExtractComponentInfo {
        framework: "react".into(),
        name: "NewComponent".into(),
        props: a.props,
        imports: a.imports.iter().map(|i| i.local.clone()).collect(),
        extension: ext.into(),
        start: a.start,
        end: a.end,
    })
}

/// Extract the selection of `path` into the component `name` at `new_path`.
pub fn extract(
    path: &str,
    content: &str,
    start: usize,
    end: usize,
    name: &str,
    new_path: &str,
) -> Result<ExtractedComponent, String> {
    check_component_name(name)?;
    let a = analyse(path, content, start, end)?;
    let parent_imports = imports::imports(content);
    if parent_imports.iter().any(|i| i.local == name) {
        return Err(format!("{name} is already defined in this file"));
    }
    let style = import_style(content, STYLE);
    let semi = if style.semicolon { ";" } else { "" };
    let unit = indent_unit(content);
    let typescript = matches!(extension(new_path), "tsx" | "ts");
    // The new component.
    let mut head: Vec<String> = Vec::new();
    // The classic JSX runtime needs React in scope.
    if let Some(react) = parent_imports.iter().find(|i| {
        i.source == "react" && i.local == "React" && i.imported != Imported::Named("React".into())
    }) {
        head.push(import_line(react, "react", style));
    }
    for i in &a.imports {
        head.push(import_line(
            i,
            &rebase_spec(&i.source, path, new_path),
            style,
        ));
    }
    let body_indent = unit.repeat(2);
    let mut markup = reindent(
        &content[a.start..a.end],
        &line_indent(content, a.start),
        &body_indent,
    );
    if a.roots > 1 {
        let inner = reindent(
            &content[a.start..a.end],
            &line_indent(content, a.start),
            &unit.repeat(3),
        );
        markup = format!("{body_indent}<>\n{inner}\n{body_indent}</>");
    }
    let mut text = String::new();
    if !head.is_empty() {
        text.push_str(&head.join("\n"));
        text.push_str("\n\n");
    }
    let params = if a.props.is_empty() {
        String::new()
    } else if typescript {
        text.push_str(&format!("interface {name}Props {{\n"));
        for p in &a.props {
            text.push_str(&format!("{unit}{p}: any;\n"));
        }
        text.push_str("}\n\n");
        format!("{{ {} }}: {name}Props", a.props.join(", "))
    } else {
        format!("{{ {} }}", a.props.join(", "))
    };
    text.push_str(&format!(
        "export default function {name}({params}) {{\n{unit}return (\n{markup}\n{unit}){semi}\n}}\n"
    ));

    // The parent: the element and the import.
    let attrs: String = a.props.iter().map(|p| format!(" {p}={{{p}}}")).collect();
    let mut edits = vec![Replacement {
        start: a.start,
        end: a.end,
        text: format!("<{name}{attrs} />"),
    }];
    let spec = relative_spec(path, new_path);
    let spec = spec
        .strip_suffix(&format!(".{}", extension(new_path)))
        .unwrap_or(&spec)
        .to_string();
    let import = default_import(name, &spec, style);
    edits.push(match after_imports(content, 0, content.len()) {
        Some(at) => Replacement {
            start: at,
            end: at,
            text: format!("\n{import}"),
        },
        None => Replacement {
            start: 0,
            end: 0,
            text: format!("{import}\n\n"),
        },
    });
    Ok(ExtractedComponent {
        path: new_path.to_string(),
        text,
        edits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, edits: &[Replacement]) -> String {
        let mut sorted: Vec<&Replacement> = edits.iter().collect();
        sorted.sort_by_key(|e| std::cmp::Reverse(e.start));
        let mut out = text.to_string();
        for e in sorted {
            out.replace_range(e.start..e.end, &e.text);
        }
        out
    }

    const CART: &str = "import { useState } from \"react\";\nimport Badge from \"./Badge\";\nimport { formatPrice } from \"../lib/money\";\n\nexport default function Cart({ items, onRemove }) {\n  const [open, setOpen] = useState(false);\n  return (\n    <section>\n      <ul className=\"cart\">\n        {items.map((item) => (\n          <li key={item.id}>\n            {item.name} <Badge>{formatPrice(item.price)}</Badge>\n            <button onClick={() => onRemove(item)}>Remove</button>\n          </li>\n        ))}\n      </ul>\n      <p>{open ? \"open\" : \"closed\"}</p>\n    </section>\n  );\n}\n";

    #[test]
    fn extracts_jsx_into_a_component_file() {
        let start = CART.find("<ul").unwrap();
        let end = CART.find("</ul>").unwrap() + 5;
        let info = prepare("src/Cart.jsx", CART, start, end).unwrap();
        assert_eq!(info.props, ["items", "onRemove"]);
        assert_eq!(info.imports, ["Badge", "formatPrice"]);
        assert_eq!(info.extension, "jsx");
        let out = extract(
            "src/Cart.jsx",
            CART,
            start,
            end,
            "CartList",
            "src/cart/CartList.jsx",
        )
        .unwrap();
        assert_eq!(
            out.text,
            "import Badge from \"../Badge\";\nimport { formatPrice } from \"../../lib/money\";\n\nexport default function CartList({ items, onRemove }) {\n  return (\n    <ul className=\"cart\">\n      {items.map((item) => (\n        <li key={item.id}>\n          {item.name} <Badge>{formatPrice(item.price)}</Badge>\n          <button onClick={() => onRemove(item)}>Remove</button>\n        </li>\n      ))}\n    </ul>\n  );\n}\n"
        );
        let parent = apply(CART, &out.edits);
        assert!(parent.contains(
            "    <section>\n      <CartList items={items} onRemove={onRemove} />\n      <p>"
        ));
        assert!(parent.contains("import { formatPrice } from \"../lib/money\";\nimport CartList from \"./cart/CartList\";\n"));
    }

    #[test]
    fn typescript_props_and_several_elements() {
        let src = "import React from 'react'\n\nexport function Total({ sum }: { sum: number }) {\n  return (\n    <div>\n      <b>{sum}</b>\n      <i>{label}</i>\n    </div>\n  )\n}\n";
        let start = src.find("<b>").unwrap();
        let end = src.find("</i>").unwrap() + 4;
        let out = extract(
            "src/Total.tsx",
            src,
            start,
            end,
            "SumLine",
            "src/SumLine.tsx",
        )
        .unwrap();
        assert_eq!(
            out.text,
            "import React from 'react'\n\ninterface SumLineProps {\n  sum: any;\n  label: any;\n}\n\nexport default function SumLine({ sum, label }: SumLineProps) {\n  return (\n    <>\n      <b>{sum}</b>\n      <i>{label}</i>\n    </>\n  )\n}\n"
        );
        assert!(apply(src, &out.edits)
            .contains("import React from 'react'\nimport SumLine from './SumLine'\n"));
    }

    #[test]
    fn refuses_what_is_not_jsx() {
        let start = CART.find("{items.map").unwrap();
        assert!(prepare("src/Cart.jsx", CART, start, start + 10).is_err());
        let li = CART.find("<li").unwrap();
        assert!(prepare("src/Cart.jsx", CART, li, li + 20).is_err());
        assert!(prepare("styles.css", CART, li, li + 20).is_err());
        let ul = CART.find("<ul").unwrap();
        let end = CART.find("</ul>").unwrap() + 5;
        assert!(extract("src/Cart.jsx", CART, ul, end, "Badge", "src/Badge.jsx").is_err());
    }
}
