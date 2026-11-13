use super::*;
use webforge_lang_web::text::byte_to_utf16;
use webforge_lang_web::SourceFile;

const BUTTON: &str = r#"import { forwardRef, type ButtonHTMLAttributes } from "react";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "ghost";
  size?: "sm" | "md";
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(({ variant = "primary", size, ...rest }, ref) => (
  <button ref={ref} className={`btn btn-${variant} btn-${size}`} {...rest} />
));
"#;

const CARD: &str = r#"import { Button } from "@/components/ui";

type ProductCardProps = {
  product: Product;
  onAddToCart: (id: string) => void;
  compact?: boolean;
};

export default function ProductCard({ product, onAddToCart, compact = false }: ProductCardProps) {
  return (
    <article className={compact ? "card compact" : "card"}>
      <h3>{product.name}</h3>
      <Button variant="primary" onClick={() => onAddToCart(product.id)}>Add to cart</Button>
    </article>
  );
}

function Price({ value }: { value: number }) {
  return <span>{value}</span>;
}
"#;

const APP: &str = r#"import { useState } from "react";
import ProductCard from "./components/ProductCard";
import * as ui from "./components/ui";
import { Header } from "./components/Header";

export const App: React.FC = () => {
  const [items] = useState<Product[]>([]);
  if (items.length < 1) return <Header title="Empty" />;
  return (
    <main>
      <Header title="Storefront" sticky />
      {items.map((p) => <ProductCard key={p.id} product={p} onAddToCart={add} />)}
      <ui.Button size="sm">More</ui.Button>
    </main>
  );
};
"#;

const HEADER: &str = r#"import React from "react";

interface Props {
  title: string;
  sticky?: boolean;
}

export class Header extends React.Component<Props> {
  render() {
    return <header>{this.props.title}</header>;
  }
}
"#;

fn sources() -> Sources {
    let f = |path: &str, content: &str| SourceFile {
        path: path.into(),
        content: content.into(),
    };
    Sources::new(
        vec![
            f("src/components/ui/Button.tsx", BUTTON),
            f(
                "src/components/ui/index.ts",
                "export { Button } from './Button';\nexport * from './Input';\n",
            ),
            f("src/components/ProductCard.tsx", CARD),
            f("src/components/Header.tsx", HEADER),
            f("src/App.tsx", APP),
            f(
                "src/utils/format.ts",
                "export function FormatPrice(v: number) { return v.toFixed(2); }\n",
            ),
        ],
        [],
    )
}

/// UTF-16 offset of the `nth` occurrence of `needle` plus `delta`.
fn at(content: &str, needle: &str, delta: usize) -> usize {
    byte_to_utf16(content, content.find(needle).unwrap() + delta)
}

#[test]
fn finds_function_class_and_forward_ref_components() {
    let s = sources();
    let index = ReactIndex::new(&s);
    let mut names: Vec<_> = index
        .components()
        .into_iter()
        .map(|c| (c.name, c.kind, c.default_export))
        .collect();
    names.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        names,
        [
            ("App".to_string(), ComponentKind::Function, false),
            ("Button".to_string(), ComponentKind::Function, false),
            ("Header".to_string(), ComponentKind::Class, false),
            ("Price".to_string(), ComponentKind::Function, false),
            ("ProductCard".to_string(), ComponentKind::Function, true),
        ]
    );
}

#[test]
fn reads_props_from_types_and_destructuring() {
    let card = scan_file("src/components/ProductCard.tsx", CARD);
    let props: Vec<_> = card[0]
        .props
        .iter()
        .map(|p| {
            (
                p.name.as_str(),
                p.required,
                p.type_text.as_deref().unwrap_or(""),
            )
        })
        .collect();
    assert_eq!(
        props,
        [
            ("product", true, "Product"),
            ("onAddToCart", true, "(id: string) => void"),
            ("compact", false, "boolean"),
        ]
    );
    assert_eq!(card[0].props[0].location.line, 3);
    assert_eq!(card[1].props[0].name, "value");

    let button = scan_file("src/components/ui/Button.tsx", BUTTON);
    let names: Vec<_> = button[0].props.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["variant", "size"]);

    let header = scan_file("src/components/Header.tsx", HEADER);
    assert_eq!(header[0].props.len(), 2);
    assert!(scan_file("src/utils/format.ts", "export function FormatPrice() {}").is_empty());
}

#[test]
fn goto_component_through_default_named_barrel_and_namespace_imports() {
    let s = sources();
    let index = ReactIndex::new(&s);
    let goto = |needle: &str, delta: usize| index.goto("src/App.tsx", APP, at(APP, needle, delta));

    let card = goto("<ProductCard", 3);
    assert_eq!(
        card,
        [Location {
            path: "src/components/ProductCard.tsx".into(),
            line: 8,
            column: 24
        }]
    );
    let header = goto("<Header title=\"Storefront\"", 2);
    assert_eq!(header[0].path, "src/components/Header.tsx");
    let button = goto("<ui.Button", 5);
    assert_eq!(button[0].path, "src/components/ui/Button.tsx");
    // In ProductCard, Button comes from the "@/components/ui" barrel.
    let from_card = index.goto(
        "src/components/ProductCard.tsx",
        CARD,
        at(CARD, "<Button", 2),
    );
    assert_eq!(from_card[0].path, "src/components/ui/Button.tsx");
    // HTML elements are left to the language server.
    assert!(goto("<main>", 2).is_empty());
}

#[test]
fn goto_prop_from_attribute() {
    let s = sources();
    let index = ReactIndex::new(&s);
    let prop = index.goto("src/App.tsx", APP, at(APP, "onAddToCart={add}", 2));
    assert_eq!(
        prop,
        [Location {
            path: "src/components/ProductCard.tsx".into(),
            line: 4,
            column: 2
        }]
    );
    let sticky = index.goto("src/App.tsx", APP, at(APP, "sticky", 1));
    assert_eq!(
        (sticky[0].path.as_str(), sticky[0].line),
        ("src/components/Header.tsx", 4)
    );
}

#[test]
fn usages_from_definition_and_counts() {
    let s = sources();
    let index = ReactIndex::new(&s);
    let header = index
        .components()
        .into_iter()
        .find(|c| c.name == "Header")
        .unwrap();
    assert_eq!(header.usages, 2);
    let usages = index.goto(
        "src/components/Header.tsx",
        HEADER,
        at(HEADER, "class Header", 7),
    );
    assert_eq!(usages.len(), 2);
    assert!(usages.iter().all(|u| u.path == "src/App.tsx"));
    let button = index
        .components()
        .into_iter()
        .find(|c| c.name == "Button")
        .unwrap();
    assert_eq!(button.usages, 2);
}

#[test]
fn unsaved_text_is_used_for_the_current_file() {
    let s = sources();
    let index = ReactIndex::new(&s);
    let edited = format!("{APP}\nconst Extra = () => <Header title=\"x\" />;\n");
    let usages = index.goto(
        "src/components/Header.tsx",
        HEADER,
        at(HEADER, "class Header", 7),
    );
    assert_eq!(usages.len(), 2);
    let target = index.goto(
        "src/App.tsx",
        &edited,
        at(&edited, "<Header title=\"x\"", 2),
    );
    assert_eq!(target[0].path, "src/components/Header.tsx");
}

const EMPTY_STATE: &str = r#"type Props = { message: string; icon?: string };

export default ({ message, icon = "cart" }: Props) => (
  <div className="empty"><i className={icon} />{message}</div>
);
"#;

const RATING: &str = r#"export default function (props: { value: number; max?: number }) {
  return <span className="rating">{props.value}</span>;
}
"#;

const SHOP: &str = r#"import EmptyState from "./components/empty-state";
import Rating from "@storefront/ui/Rating";
import { Avatar } from "@storefront/ui";

export function Shop() {
  return (
    <section>
      <EmptyState message="Nothing here" icon="box" />
      <Rating value={4} />
      <Avatar name="Ada" />
    </section>
  );
}
"#;

fn workspace_sources() -> Sources {
    let f = |path: &str, content: &str| SourceFile {
        path: path.into(),
        content: content.into(),
    };
    Sources::new(
        vec![
            f(
                "package.json",
                r#"{ "name": "storefront", "workspaces": ["packages/*"] }"#,
            ),
            f("src/components/empty-state.tsx", EMPTY_STATE),
            f("src/Shop.tsx", SHOP),
            f(
                "packages/ui/package.json",
                r#"{ "name": "@storefront/ui", "main": "dist/index.js", "types": "src/index.ts" }"#,
            ),
            f(
                "packages/ui/src/index.ts",
                "export { Avatar } from './Avatar';\n",
            ),
            f(
                "packages/ui/src/Avatar.tsx",
                "export const Avatar = ({ name }: { name: string }) => <b>{name}</b>;\n",
            ),
            f("packages/ui/src/Rating/index.tsx", RATING),
            f(
                "src/memo.tsx",
                "import { memo } from 'react';\nexport default memo(() => <hr />);\n",
            ),
            f("src/utils/math.ts", "export default () => 42;\n"),
        ],
        [],
    )
}

#[test]
fn anonymous_default_exports_are_named_after_their_file() {
    assert_eq!(
        name_from_file("src/components/empty-state.tsx"),
        "EmptyState"
    );
    assert_eq!(name_from_file("packages/ui/src/Rating/index.tsx"), "Rating");
    assert_eq!(name_from_file("src/user_card.stories.tsx"), "UserCard");

    let s = workspace_sources();
    let index = ReactIndex::new(&s);
    let names: Vec<String> = index.components().into_iter().map(|c| c.name).collect();
    for name in ["EmptyState", "Rating", "Avatar", "Memo"] {
        assert!(names.contains(&name.to_string()), "{name} in {names:?}");
    }
    // A default-exported arrow function in a file without JSX is not a component.
    assert!(!names.contains(&"Math".to_string()), "{names:?}");

    let empty = index
        .components()
        .into_iter()
        .find(|c| c.name == "EmptyState")
        .unwrap();
    assert!(empty.default_export);
    assert_eq!((empty.location.line, empty.location.column), (2, 0));
    let props: Vec<&str> = empty.props.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(props, ["message", "icon"]);
    assert_eq!(empty.usages, 1);
    let rating = index
        .components()
        .into_iter()
        .find(|c| c.name == "Rating")
        .unwrap();
    let props: Vec<&str> = rating.props.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(props, ["value", "max"]);

    // Go to Declaration on the tag and its props; on `export` its usages.
    let goto =
        |needle: &str, delta: usize| index.goto("src/Shop.tsx", SHOP, at(SHOP, needle, delta));
    let t = goto("<EmptyState", 2);
    assert_eq!(
        (t[0].path.as_str(), t[0].line),
        ("src/components/empty-state.tsx", 2)
    );
    let t = goto("icon=\"box\"", 1);
    assert_eq!(
        (t[0].path.as_str(), t[0].line),
        ("src/components/empty-state.tsx", 0)
    );
    let usages = index.goto(
        "src/components/empty-state.tsx",
        EMPTY_STATE,
        at(EMPTY_STATE, "export default", 2),
    );
    assert_eq!(usages.len(), 1);
    assert_eq!(usages[0].path, "src/Shop.tsx");
}

#[test]
fn tags_resolve_into_workspace_packages() {
    let s = workspace_sources();
    let index = ReactIndex::new(&s);
    let goto =
        |needle: &str, delta: usize| index.goto("src/Shop.tsx", SHOP, at(SHOP, needle, delta));
    // `@storefront/ui` -> `types: src/index.ts` (dist/ is not built) -> barrel -> Avatar.
    let t = goto("<Avatar", 2);
    assert_eq!(t[0].path, "packages/ui/src/Avatar.tsx");
    // A subpath without `exports`: the package's `src/Rating/index.tsx`.
    let t = goto("<Rating", 2);
    assert_eq!(t[0].path, "packages/ui/src/Rating/index.tsx");
    let t = goto("value={4}", 1);
    assert_eq!(
        (t[0].path.as_str(), t[0].line),
        ("packages/ui/src/Rating/index.tsx", 0)
    );
}
