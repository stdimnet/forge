//! Parser of markup abbreviations (`ul#nav>li.item$*3>a{Item $}`) into a tree.
//!
//! Syntax: child `>`, sibling `+`, climb-up `^`, multiplication `*N` (a bare `*` repeats
//! once per wrapped line), grouping `( )`, id `#`, class `.`, attributes `[a=b c="d" e.]`,
//! text `{ }`, numbering `$`, `$$`, `$@-`, `$@3`, self-closing `/`. Names may carry `:` and
//! `!` (`input:checkbox`, `!`) and a trailing `+` for the expanding snippets (`ul+`).

/// An attribute as written: `[href]`, `[title="x"]`, `[disabled.]`, `[onClick={go}]`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Attr {
    pub name: String,
    /// `None` for `[href]` (an empty value with a tab stop).
    pub value: Option<String>,
    /// `[disabled.]`: written without a value.
    pub boolean: bool,
    /// `[onClick={go}]`: a JSX expression, written without quotes.
    pub expression: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    /// `*3`
    Count(usize),
    /// `*` without a count: once per line of the wrapped text.
    Implicit,
}

/// One element: `div#main.a.b[title=x]{text}`. An empty `name` is an implicit tag (or a
/// text node when only `text` is set).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Element {
    pub name: String,
    /// Attributes in the written order; `#id` and `.class` are `id` / `class` entries
    /// (classes collect in one `class` attribute).
    pub attrs: Vec<Attr>,
    pub text: Option<String>,
    pub self_closing: bool,
}

impl Element {
    /// A text node: `{Hello}`.
    pub fn is_text(&self) -> bool {
        self.name.is_empty() && self.attrs.is_empty() && self.text.is_some()
    }

    pub(crate) fn add_class(&mut self, class: &str) {
        match self.attrs.iter_mut().find(|a| a.name == "class") {
            Some(a) => {
                let v = a.value.get_or_insert_with(String::new);
                if !v.is_empty() {
                    v.push(' ');
                }
                v.push_str(class);
            }
            None => self.attrs.push(Attr {
                name: "class".into(),
                value: Some(class.into()),
                ..Attr::default()
            }),
        }
    }

    pub(crate) fn set_attr(&mut self, attr: Attr) {
        match self.attrs.iter_mut().find(|a| a.name == attr.name) {
            Some(a) => *a = attr,
            None => self.attrs.push(attr),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Root,
    Group,
    Element(Element),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub kind: Kind,
    pub repeat: Option<Repeat>,
    pub parent: usize,
    pub children: Vec<usize>,
}

/// The parsed abbreviation; `nodes[0]` is the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    pub nodes: Vec<Node>,
}

impl Tree {
    pub fn root(&self) -> &Node {
        &self.nodes[0]
    }

    pub fn has_implicit_repeat(&self) -> bool {
        self.nodes
            .iter()
            .any(|n| n.repeat == Some(Repeat::Implicit))
    }

    /// Elements in the tree (for "does this look like markup" checks).
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.nodes.iter().filter_map(|n| match &n.kind {
            Kind::Element(e) => Some(e),
            _ => None,
        })
    }
}

/// Snippet names that end in `+` (`ul+` is `ul>li`).
const PLUS_NAMES: &[&str] = &[
    "ol", "ul", "dl", "map", "table", "colgroup", "colg", "tr", "select", "optgroup", "optg", "pic",
];

/// Parse an abbreviation; `Err` describes the first problem.
pub fn parse(abbr: &str) -> Result<Tree, String> {
    let mut p = Parser {
        chars: abbr.chars().collect(),
        pos: 0,
        tree: Tree {
            nodes: vec![Node {
                kind: Kind::Root,
                repeat: None,
                parent: 0,
                children: Vec::new(),
            }],
        },
    };
    if p.chars.is_empty() {
        return Err("empty abbreviation".into());
    }
    p.seq(0)?;
    if p.pos < p.chars.len() {
        return Err(format!("unexpected '{}'", p.chars[p.pos]));
    }
    Ok(p.tree)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    tree: Tree,
}

const MAX_NODES: usize = 2000;

fn is_operator(c: char) -> bool {
    matches!(c, '>' | '+' | '^' | ')' | '*' | '(')
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '-' | '_' | ':' | '!' | '$' | '@')
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }

    fn add(&mut self, kind: Kind) -> Result<usize, String> {
        if self.tree.nodes.len() >= MAX_NODES {
            return Err("abbreviation too long".into());
        }
        self.tree.nodes.push(Node {
            kind,
            repeat: None,
            parent: 0,
            children: Vec::new(),
        });
        Ok(self.tree.nodes.len() - 1)
    }

    fn attach(&mut self, parent: usize, child: usize) {
        self.tree.nodes[child].parent = parent;
        self.tree.nodes[parent].children.push(child);
    }

    /// Items joined by operators, attached below `base`.
    fn seq(&mut self, base: usize) -> Result<(), String> {
        let mut ctx = base;
        loop {
            let item = self.item()?;
            self.attach(ctx, item);
            match self.peek() {
                None | Some(')') => return Ok(()),
                Some('>') => {
                    self.pos += 1;
                    ctx = self.child_target(item);
                }
                Some('+') => self.pos += 1,
                Some('^') => {
                    while self.peek() == Some('^') {
                        self.pos += 1;
                        if ctx != base {
                            ctx = self.tree.nodes[ctx].parent;
                        }
                    }
                }
                Some(c) => return Err(format!("unexpected '{c}'")),
            }
            if matches!(self.peek(), None | Some(')')) {
                return Err("abbreviation ends with an operator".into());
            }
        }
    }

    /// Where `x>…` attaches children: the element itself, or a group's last item.
    fn child_target(&self, item: usize) -> usize {
        let mut id = item;
        while matches!(self.tree.nodes[id].kind, Kind::Group) {
            match self.tree.nodes[id].children.last() {
                Some(&last) => id = last,
                None => break,
            }
        }
        id
    }

    fn item(&mut self) -> Result<usize, String> {
        if self.peek() == Some('(') {
            self.pos += 1;
            let group = self.add(Kind::Group)?;
            self.seq(group)?;
            if self.peek() != Some(')') {
                return Err("missing ')'".into());
            }
            self.pos += 1;
            self.tree.nodes[group].repeat = self.repeat()?;
            return Ok(group);
        }
        let mut el = Element {
            name: self.name(),
            ..Element::default()
        };
        let mut any = !el.name.is_empty();
        loop {
            match self.peek() {
                Some('.') => {
                    self.pos += 1;
                    let class = self.word()?;
                    el.add_class(&class);
                }
                Some('#') => {
                    self.pos += 1;
                    let id = self.word()?;
                    el.set_attr(Attr {
                        name: "id".into(),
                        value: Some(id),
                        ..Attr::default()
                    });
                }
                Some('[') => {
                    self.pos += 1;
                    for a in self.attributes()? {
                        if a.name == "class" {
                            if let Some(v) = &a.value {
                                el.add_class(v);
                                continue;
                            }
                        }
                        el.set_attr(a);
                    }
                }
                Some('{') => {
                    self.pos += 1;
                    let t = self.text()?;
                    el.text = Some(el.text.take().unwrap_or_default() + &t);
                }
                _ => break,
            }
            any = true;
        }
        if !any {
            return Err(match self.peek() {
                Some(c) => format!("unexpected '{c}'"),
                None => "expected an element".into(),
            });
        }
        let id = self.add(Kind::Element(el))?;
        if self.closing_slash() {
            self.set_self_closing(id);
        }
        self.tree.nodes[id].repeat = self.repeat()?;
        if self.closing_slash() {
            self.set_self_closing(id);
        }
        Ok(id)
    }

    fn set_self_closing(&mut self, id: usize) {
        if let Kind::Element(e) = &mut self.tree.nodes[id].kind {
            e.self_closing = true;
        }
    }

    /// A `/` that ends an element (`br/`, `img/*2`).
    fn closing_slash(&mut self) -> bool {
        if self.peek() == Some('/') && self.peek_at(1).is_none_or(is_operator) {
            self.pos += 1;
            return true;
        }
        false
    }

    fn repeat(&mut self) -> Result<Option<Repeat>, String> {
        if self.peek() != Some('*') {
            return Ok(None);
        }
        self.pos += 1;
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        if start == self.pos {
            return Ok(Some(Repeat::Implicit));
        }
        let n: String = self.chars[start..self.pos].iter().collect();
        let n: usize = n.parse().map_err(|_| "bad count".to_string())?;
        Ok(Some(Repeat::Count(n)))
    }

    /// Element name, with a `+` of the expanding snippets (`ul+`).
    fn name(&mut self) -> String {
        let start = self.pos;
        // Names start with a letter (`div`, `h$`, `input:c`) or are `!` snippets.
        if self.peek().is_some_and(|c| c.is_alphabetic() || c == '!') {
            while let Some(c) = self.peek() {
                let after_dollar = self.pos > start && self.chars[self.pos - 1] == '$';
                if !is_name_char(c) || (c == '@' && !after_dollar) {
                    break;
                }
                self.pos += 1;
            }
        }
        let mut name: String = self.chars[start..self.pos].iter().collect();
        if self.peek() == Some('+')
            && PLUS_NAMES.contains(&name.as_str())
            && self
                .peek_at(1)
                .is_none_or(|c| matches!(c, '>' | '^' | ')' | '*' | '+'))
        {
            self.pos += 1;
            name.push('+');
        }
        name
    }

    /// Class or id: up to the next `.`, `#`, `[`, `{`, operator or whitespace.
    fn word(&mut self) -> Result<String, String> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if matches!(c, '.' | '#' | '[' | '{' | '>' | '+' | '^' | '*' | '(' | ')')
                || c.is_whitespace()
            {
                break;
            }
            if c == '/' && self.peek_at(1).is_none_or(is_operator) {
                break;
            }
            self.pos += 1;
        }
        if start == self.pos {
            return Err("empty class or id".into());
        }
        Ok(self.chars[start..self.pos].iter().collect())
    }

    /// `{…}` text after the opening brace; nested braces are kept, `\}` is a brace.
    fn text(&mut self) -> Result<String, String> {
        let mut depth = 0;
        let mut out = String::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                '\\' => {
                    if let Some(n) = self.peek() {
                        self.pos += 1;
                        out.push(n);
                    }
                }
                '{' => {
                    depth += 1;
                    out.push(c);
                }
                '}' if depth == 0 => return Ok(out),
                '}' => {
                    depth -= 1;
                    out.push(c);
                }
                _ => out.push(c),
            }
        }
        Err("missing '}'".into())
    }

    /// `[…]` after the opening bracket.
    fn attributes(&mut self) -> Result<Vec<Attr>, String> {
        let mut out = Vec::new();
        loop {
            while self.peek().is_some_and(char::is_whitespace) {
                self.pos += 1;
            }
            match self.peek() {
                None => return Err("missing ']'".into()),
                Some(']') => {
                    self.pos += 1;
                    return Ok(out);
                }
                _ => {}
            }
            let start = self.pos;
            while self
                .peek()
                .is_some_and(|c| c != '=' && c != ']' && !c.is_whitespace())
            {
                self.pos += 1;
            }
            let mut name: String = self.chars[start..self.pos].iter().collect();
            if name.is_empty() {
                return Err("attribute without a name".into());
            }
            let mut attr = Attr::default();
            if name.len() > 1 && name.ends_with('.') {
                name.pop();
                attr.boolean = true;
            }
            if self.peek() == Some('=') {
                self.pos += 1;
                match self.peek() {
                    Some(q @ ('"' | '\'')) => {
                        self.pos += 1;
                        let s = self.pos;
                        while self.peek().is_some_and(|c| c != q) {
                            self.pos += 1;
                        }
                        if self.peek().is_none() {
                            return Err("missing quote".into());
                        }
                        attr.value = Some(self.chars[s..self.pos].iter().collect());
                        self.pos += 1;
                    }
                    Some('{') => {
                        self.pos += 1;
                        let t = self.text()?;
                        attr.value = Some(t);
                        attr.expression = true;
                    }
                    _ => {
                        let s = self.pos;
                        while self.peek().is_some_and(|c| c != ']' && !c.is_whitespace()) {
                            self.pos += 1;
                        }
                        attr.value = Some(self.chars[s..self.pos].iter().collect());
                    }
                }
                attr.boolean = false;
            }
            attr.name = name;
            out.push(attr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(t: &Tree, id: usize) -> &Element {
        match &t.nodes[id].kind {
            Kind::Element(e) => e,
            k => panic!("not an element: {k:?}"),
        }
    }

    fn names(t: &Tree, id: usize) -> Vec<String> {
        t.nodes[id]
            .children
            .iter()
            .map(|&c| match &t.nodes[c].kind {
                Kind::Element(e) => e.name.clone(),
                Kind::Group => "()".into(),
                Kind::Root => "root".into(),
            })
            .collect()
    }

    #[test]
    fn operators_build_the_tree() {
        let t = parse("div>p+ul>li^footer").unwrap();
        assert_eq!(names(&t, 0), ["div"]);
        let div = t.root().children[0];
        assert_eq!(names(&t, div), ["p", "ul", "footer"]);
        let ul = t.nodes[div].children[1];
        assert_eq!(names(&t, ul), ["li"]);
    }

    #[test]
    fn climbs_several_levels_but_not_above_the_root() {
        let t = parse("a>b>c^^^d").unwrap();
        assert_eq!(names(&t, 0), ["a", "d"]);
    }

    #[test]
    fn groups_and_repeats() {
        let t = parse("(div>dl>(dt+dd)*3)+footer>p").unwrap();
        assert_eq!(names(&t, 0), ["()", "footer"]);
        let g = t.root().children[0];
        assert!(t.nodes[g].repeat.is_none());
        let dl = t.nodes[t.nodes[g].children[0]].children[0];
        let inner = t.nodes[dl].children[0];
        assert_eq!(t.nodes[inner].repeat, Some(Repeat::Count(3)));
        assert_eq!(names(&t, inner), ["dt", "dd"]);
    }

    #[test]
    fn group_children_attach_to_its_last_item() {
        let t = parse("(a+b)>c").unwrap();
        let g = t.root().children[0];
        let b = t.nodes[g].children[1];
        assert_eq!(names(&t, b), ["c"]);
    }

    #[test]
    fn ids_classes_attributes_and_text() {
        let t = parse(r#"a#top.btn.btn-lg[href="/x y" target=_blank data-on disabled.]{Go $}"#)
            .unwrap();
        let a = el(&t, 1);
        assert_eq!(a.name, "a");
        let attrs: Vec<(&str, Option<&str>, bool)> = a
            .attrs
            .iter()
            .map(|x| (x.name.as_str(), x.value.as_deref(), x.boolean))
            .collect();
        assert_eq!(
            attrs,
            [
                ("id", Some("top"), false),
                ("class", Some("btn btn-lg"), false),
                ("href", Some("/x y"), false),
                ("target", Some("_blank"), false),
                ("data-on", None, false),
                ("disabled", None, true),
            ]
        );
        assert_eq!(a.text.as_deref(), Some("Go $"));
    }

    #[test]
    fn text_nodes_and_nested_braces() {
        let t = parse("{Click {here}}+a").unwrap();
        assert!(el(&t, 1).is_text());
        assert_eq!(el(&t, 1).text.as_deref(), Some("Click {here}"));
        let t = parse(r"p{a\}b}").unwrap();
        assert_eq!(el(&t, 1).text.as_deref(), Some("a}b"));
    }

    #[test]
    fn jsx_expressions_in_attributes() {
        let t = parse("button[onClick={() => go(1)}]").unwrap();
        let a = &el(&t, 1).attrs[0];
        assert!(a.expression);
        assert_eq!(a.value.as_deref(), Some("() => go(1)"));
    }

    #[test]
    fn plus_snippets_and_names_with_colons() {
        let t = parse("ul+").unwrap();
        assert_eq!(el(&t, 1).name, "ul+");
        let t = parse("ul+p").unwrap();
        assert_eq!(names(&t, 0), ["ul", "p"]);
        let t = parse("input:checkbox+!").unwrap();
        assert_eq!(names(&t, 0), ["input:checkbox", "!"]);
    }

    #[test]
    fn numbering_and_implicit_repeat() {
        let t = parse("li.item$$@-3*5").unwrap();
        assert_eq!(el(&t, 1).attrs[0].value.as_deref(), Some("item$$@-3"));
        assert_eq!(t.nodes[1].repeat, Some(Repeat::Count(5)));
        let t = parse("ul>li*").unwrap();
        assert!(t.has_implicit_repeat());
    }

    #[test]
    fn self_closing_and_tailwind_classes() {
        let t = parse("div/").unwrap();
        assert!(el(&t, 1).self_closing);
        let t = parse("div.w-1/2.md:flex").unwrap();
        assert_eq!(el(&t, 1).attrs[0].value.as_deref(), Some("w-1/2 md:flex"));
    }

    #[test]
    fn errors() {
        for bad in [
            "", "div>", "div+", "(div", "div)", "a[href", "p{x", ".", "div>>p", "*3",
        ] {
            assert!(parse(bad).is_err(), "{bad:?} should fail");
        }
    }
}
