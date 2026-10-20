//! Structural Lua parser: a tolerant recursive-descent pass over the tokens that records
//! declarations (locals, parameters, globals, fields, functions and methods) with their
//! scopes, name and field references, `require` calls, the module's `return` value and the
//! doc comments attached to each statement. It does not build a full syntax tree.

use std::collections::HashMap;

use crate::annotations::{parse_block, DocBlock};
use crate::lexer::{lex, string_value, Comment, Tok, Token};
use crate::model::*;
use crate::offsets::LineIndex;

/// Index one Lua file.
pub fn parse(path: &str, src: &str) -> FileIndex {
    let (toks, comments) = lex(src);
    let mut p = Parser {
        src,
        toks,
        p: 0,
        lines: LineIndex::new(src),
        docs: Vec::new(),
        doc_at: HashMap::new(),
        scopes: vec![Vec::new()],
        tables: 0,
        fn_depth: 0,
        fn_doc: None,
        out: FileIndex {
            path: path.to_string(),
            ..FileIndex::default()
        },
    };
    p.collect_docs(&comments);
    p.out.first_statement = p.toks.first().map(|t| t.start);
    p.block();
    // Anything left (stray `end`s) is skipped statement by statement.
    while p.p < p.toks.len() {
        p.p += 1;
        p.block();
    }
    p.pop_scope(src.len());
    p.finish();
    p.out
}

/// Result of parsing an expression.
#[derive(Debug, Clone)]
struct Expr {
    value: Value,
    /// Parameters, when the expression is a function.
    params: Option<Vec<String>>,
    /// Value and byte range, when the expression is a single string literal.
    string: Option<(String, usize, usize)>,
}

impl Expr {
    fn of(value: Value) -> Self {
        Expr {
            value,
            params: None,
            string: None,
        }
    }
}

/// The left-hand side of an assignment.
enum LTarget {
    Name {
        name: String,
        start: usize,
        end: usize,
        local: Option<usize>,
    },
    Field {
        owner: Value,
        container: String,
        name: String,
        start: usize,
        end: usize,
    },
    Other,
}

struct Parser<'a> {
    src: &'a str,
    toks: Vec<Token>,
    p: usize,
    lines: LineIndex,
    docs: Vec<DocBlock>,
    /// Token start offset -> doc block attached to the statement starting there.
    doc_at: HashMap<usize, usize>,
    scopes: Vec<Vec<usize>>,
    tables: usize,
    fn_depth: usize,
    /// Doc block of the statement whose next function expression it describes.
    fn_doc: Option<DocBlock>,
    out: FileIndex,
}

impl<'a> Parser<'a> {
    // ---- Doc comments ---------------------------------------------------------------

    fn collect_docs(&mut self, comments: &[Comment]) {
        let src = self.src;
        let mut groups: Vec<Vec<&Comment>> = Vec::new();
        for c in comments.iter().filter(|c| c.doc) {
            let joins = groups.last().and_then(|g| g.last()).is_some_and(|prev| {
                let gap = &src[prev.end..c.start];
                gap.trim().is_empty() && gap.matches('\n').count() == 1
            });
            if joins {
                groups.last_mut().expect("group").push(c);
            } else {
                groups.push(vec![c]);
            }
        }
        for group in groups {
            let lines: Vec<(&str, usize)> =
                group.iter().map(|c| (c.body(src), c.start + 3)).collect();
            let mut doc = parse_block(&lines);
            doc.start = group[0].start;
            doc.end = group[group.len() - 1].end;
            self.out.doc_ranges.push((doc.start, doc.end));
            for class in &doc.classes {
                self.out.classes.push(ClassDef {
                    ann: class.clone(),
                    pos: self.lines.pos(src, class.offset),
                    decl: None,
                    doc: doc.description.clone(),
                });
            }
            for alias in &doc.aliases {
                self.out.aliases.push(AliasDef {
                    ann: alias.clone(),
                    pos: self.lines.pos(src, alias.offset),
                });
            }
            // Attach to the statement that follows directly (no blank line in between).
            let next = self.toks.iter().find(|t| t.start >= doc.end);
            if let Some(t) = next {
                if src[doc.end..t.start].matches('\n').count() <= 1 {
                    self.doc_at.insert(t.start, self.docs.len());
                }
            }
            self.docs.push(doc);
        }
    }

    fn take_doc(&self) -> Option<DocBlock> {
        let t = self.toks.get(self.p)?;
        self.doc_at.get(&t.start).map(|&i| self.docs[i].clone())
    }

    // ---- Tokens ---------------------------------------------------------------------

    fn peek(&self) -> Option<Token> {
        self.toks.get(self.p).copied()
    }

    fn peek_at(&self, n: usize) -> Option<Token> {
        self.toks.get(self.p + n).copied()
    }

    fn text(&self, t: Token) -> &'a str {
        &self.src[t.start..t.end]
    }

    fn is(&self, s: &str) -> bool {
        self.peek()
            .is_some_and(|t| t.kind != Tok::Str && self.text(t) == s)
    }

    fn eat(&mut self, s: &str) -> bool {
        if self.is(s) {
            self.p += 1;
            true
        } else {
            false
        }
    }

    fn name(&mut self) -> Option<Token> {
        let t = self.peek()?;
        if t.kind == Tok::Name {
            self.p += 1;
            Some(t)
        } else {
            None
        }
    }

    fn prev_end(&self) -> usize {
        self.p
            .checked_sub(1)
            .and_then(|i| self.toks.get(i))
            .map(|t| t.end)
            .unwrap_or(0)
    }

    fn here(&self) -> usize {
        self.peek().map(|t| t.start).unwrap_or(self.src.len())
    }

    fn at_block_end(&self) -> bool {
        match self.peek() {
            None => true,
            Some(t) => {
                t.kind == Tok::Keyword
                    && matches!(self.text(t), "end" | "else" | "elseif" | "until")
            }
        }
    }

    // ---- Scopes and declarations ----------------------------------------------------

    fn push_scope(&mut self) {
        self.scopes.push(Vec::new());
    }

    fn pop_scope(&mut self, end: usize) {
        if let Some(scope) = self.scopes.pop() {
            for i in scope {
                self.out.decls[i].scope.1 = end;
            }
        }
    }

    fn lookup(&self, name: &str) -> Option<usize> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|s| s.iter().rev())
            .copied()
            .find(|&i| self.out.decls[i].name == name)
    }

    fn resolve(&self, name: &str) -> Value {
        match self.lookup(name) {
            Some(i) => Value::Local(i),
            None => Value::Global(name.to_string()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn decl(
        &mut self,
        name: &str,
        kind: DeclKind,
        start: usize,
        end: usize,
        value: Value,
        owner: Option<Value>,
        container: String,
        doc: Option<DocBlock>,
    ) -> usize {
        let idx = self.out.decls.len();
        self.out.decls.push(Decl {
            name: name.to_string(),
            kind,
            start,
            end,
            pos: self.lines.pos(self.src, start),
            scope: (end, usize::MAX),
            owner,
            container,
            value,
            params: Vec::new(),
            doc,
            class: None,
            top_level: self.fn_depth == 0,
            attrib: None,
        });
        if kind.is_local() {
            if let Some(scope) = self.scopes.last_mut() {
                scope.push(idx);
            }
        }
        idx
    }

    fn bind_class(&mut self, decl: usize, doc: &Option<DocBlock>) {
        let Some(class) = doc.as_ref().and_then(|d| d.classes.first()) else {
            return;
        };
        self.out.decls[decl].class = Some(class.name.clone());
        if let Some(c) = self
            .out
            .classes
            .iter_mut()
            .find(|c| c.ann.offset == class.offset)
        {
            c.decl = Some(decl);
        }
    }

    fn reference(&mut self, start: usize, end: usize, target: Target) {
        self.out.refs.push(Ref { start, end, target });
    }

    // ---- Statements -----------------------------------------------------------------

    fn block(&mut self) {
        while !self.at_block_end() {
            let before = self.p;
            self.statement();
            if self.p == before {
                self.p += 1;
            }
        }
    }

    fn scoped_block(&mut self) {
        self.push_scope();
        self.block();
        let end = self.here();
        self.pop_scope(end);
    }

    fn statement(&mut self) {
        let Some(t) = self.peek() else { return };
        let doc = self.take_doc();
        if t.kind == Tok::Keyword || t.kind == Tok::Op {
            match self.text(t) {
                ";" | "break" => self.p += 1,
                "::" => {
                    self.p += 1;
                    self.name();
                    self.eat("::");
                }
                "goto" => {
                    self.p += 1;
                    self.name();
                }
                "do" => {
                    self.p += 1;
                    self.scoped_block();
                    self.eat("end");
                }
                "while" => {
                    self.p += 1;
                    self.expr();
                    self.eat("do");
                    self.scoped_block();
                    self.eat("end");
                }
                "repeat" => {
                    self.p += 1;
                    self.push_scope();
                    self.block();
                    self.eat("until");
                    self.expr();
                    let end = self.prev_end();
                    self.pop_scope(end);
                }
                "if" => {
                    self.p += 1;
                    self.expr();
                    self.eat("then");
                    self.scoped_block();
                    loop {
                        if self.eat("elseif") {
                            self.expr();
                            self.eat("then");
                            self.scoped_block();
                        } else if self.eat("else") {
                            self.scoped_block();
                        } else {
                            break;
                        }
                    }
                    self.eat("end");
                }
                "for" => self.for_statement(),
                "function" => self.function_statement(doc),
                "local" => self.local_statement(doc),
                "return" => {
                    self.p += 1;
                    let values = if self.at_block_end() || self.is(";") {
                        Vec::new()
                    } else {
                        self.expr_list()
                    };
                    self.eat(";");
                    if self.fn_depth == 0 {
                        self.out.returns = values.into_iter().next().map(|e| e.value);
                    }
                }
                _ => self.expr_statement(doc),
            }
        } else {
            self.expr_statement(doc);
        }
    }

    fn for_statement(&mut self) {
        self.p += 1;
        let mut names = Vec::new();
        while let Some(n) = self.name() {
            names.push(n);
            if !self.eat(",") {
                break;
            }
        }
        if self.eat("=") {
            self.expr_list();
        } else {
            self.eat("in");
            self.expr_list();
        }
        self.eat("do");
        self.push_scope();
        for n in names {
            let name = self.text(n);
            self.decl(
                name,
                DeclKind::ForVar,
                n.start,
                n.end,
                Value::Unknown,
                None,
                String::new(),
                None,
            );
        }
        self.block();
        let end = self.here();
        self.pop_scope(end);
        self.eat("end");
    }

    fn function_statement(&mut self, doc: Option<DocBlock>) {
        self.p += 1;
        let Some(first) = self.name() else { return };
        let first_name = self.text(first).to_string();
        let mut segments = vec![first];
        let mut method = false;
        while self.is(".") || self.is(":") {
            method = self.is(":");
            self.p += 1;
            match self.name() {
                Some(n) => segments.push(n),
                None => break,
            }
            if method {
                break;
            }
        }
        if segments.len() == 1 {
            // `function f()`: a global, or a local declared earlier.
            let idx = match self.lookup(&first_name) {
                Some(local) => {
                    self.reference(first.start, first.end, Target::Local(local));
                    self.out.decls[local].value = Value::Function;
                    if self.out.decls[local].doc.is_none() {
                        self.out.decls[local].doc = doc.clone();
                    }
                    local
                }
                None => self.decl(
                    &first_name,
                    DeclKind::GlobalFunction,
                    first.start,
                    first.end,
                    Value::Function,
                    None,
                    String::new(),
                    doc.clone(),
                ),
            };
            let params = self.func_body(None, doc);
            self.out.decls[idx].params = params;
            return;
        }
        // `function a.b.c()` / `function a.b:c()`.
        let mut owner = self.resolve(&first_name);
        self.reference(first.start, first.end, target_of(&owner, &first_name));
        for seg in &segments[1..segments.len() - 1] {
            let name = self.text(*seg).to_string();
            self.reference(
                seg.start,
                seg.end,
                Target::Field(owner.clone(), name.clone()),
            );
            owner = Value::Field(Box::new(owner), name);
        }
        let last = segments[segments.len() - 1];
        let container = self.container_text(first.start, segments[segments.len() - 2].end, &owner);
        let name = self.text(last).to_string();
        self.reference(
            last.start,
            last.end,
            Target::Field(owner.clone(), name.clone()),
        );
        let kind = if method {
            DeclKind::Method
        } else {
            DeclKind::FieldFunction
        };
        let idx = self.decl(
            &name,
            kind,
            last.start,
            last.end,
            Value::Function,
            Some(owner.clone()),
            container.clone(),
            doc.clone(),
        );
        let self_owner = method.then_some((owner, container, last));
        let params = self.func_body(self_owner, doc);
        self.out.decls[idx].params = params;
    }

    /// `M`, `love.graphics`; `self` inside a method shows the method's table.
    fn container_text(&self, start: usize, end: usize, owner: &Value) -> String {
        if let Value::Local(i) = owner {
            let d = &self.out.decls[*i];
            if d.kind == DeclKind::Param && d.name == "self" {
                return d.container.clone();
            }
        }
        self.src[start..end].to_string()
    }

    fn local_statement(&mut self, doc: Option<DocBlock>) {
        self.p += 1;
        if self.eat("function") {
            let Some(n) = self.name() else { return };
            let name = self.text(n).to_string();
            let idx = self.decl(
                &name,
                DeclKind::LocalFunction,
                n.start,
                n.end,
                Value::Function,
                None,
                String::new(),
                doc.clone(),
            );
            let params = self.func_body(None, doc);
            self.out.decls[idx].params = params;
            return;
        }
        let mut names = Vec::new();
        while let Some(n) = self.name() {
            let mut attrib = None;
            if self.eat("<") {
                attrib = self.name().map(|a| self.text(a).to_string());
                self.eat(">");
            }
            names.push((n, attrib));
            if !self.eat(",") {
                break;
            }
        }
        let values = if self.eat("=") {
            self.fn_doc = doc.clone();
            let v = self.expr_list();
            self.fn_doc = None;
            v
        } else {
            Vec::new()
        };
        for (i, (n, attrib)) in names.into_iter().enumerate() {
            let e = values.get(i);
            let name = self.text(n).to_string();
            let value = e.map(|e| e.value.clone()).unwrap_or(Value::Unknown);
            let d = if i == 0 { doc.clone() } else { None };
            let kind = if value == Value::Function {
                DeclKind::LocalFunction
            } else {
                DeclKind::Local
            };
            let idx = self.decl(&name, kind, n.start, n.end, value, None, String::new(), d);
            self.out.decls[idx].attrib = attrib;
            if let Some(params) = e.and_then(|e| e.params.clone()) {
                self.out.decls[idx].params = params;
            }
            if i == 0 {
                self.bind_class(idx, &doc);
            }
        }
    }

    fn expr_statement(&mut self, doc: Option<DocBlock>) {
        let mut targets = vec![self.suffixed().1];
        while self.eat(",") {
            targets.push(self.suffixed().1);
        }
        if !self.eat("=") {
            return;
        }
        self.fn_doc = doc.clone();
        let values = self.expr_list();
        self.fn_doc = None;
        for (i, target) in targets.into_iter().enumerate() {
            let e = values.get(i);
            let value = e.map(|e| e.value.clone()).unwrap_or(Value::Unknown);
            let params = e.and_then(|e| e.params.clone()).unwrap_or_default();
            let d = if i == 0 { doc.clone() } else { None };
            match target {
                LTarget::Name {
                    name,
                    start,
                    end,
                    local: Some(local),
                } => {
                    let decl = &mut self.out.decls[local];
                    if decl.value == Value::Unknown {
                        decl.value = value;
                        decl.params = params;
                    }
                    let _ = (name, start, end);
                }
                LTarget::Name {
                    name,
                    start,
                    end,
                    local: None,
                } => {
                    let kind = if value == Value::Function {
                        DeclKind::GlobalFunction
                    } else {
                        DeclKind::Global
                    };
                    let idx = self.decl(
                        &name,
                        kind,
                        start,
                        end,
                        value,
                        None,
                        String::new(),
                        d.clone(),
                    );
                    self.out.decls[idx].params = params;
                    if i == 0 {
                        self.bind_class(idx, &d);
                    }
                }
                LTarget::Field {
                    owner,
                    container,
                    name,
                    start,
                    end,
                } => {
                    let kind = if value == Value::Function {
                        DeclKind::FieldFunction
                    } else {
                        DeclKind::Field
                    };
                    let idx = self.decl(
                        &name,
                        kind,
                        start,
                        end,
                        value,
                        Some(owner),
                        container,
                        d.clone(),
                    );
                    self.out.decls[idx].params = params;
                    if i == 0 {
                        self.bind_class(idx, &d);
                    }
                }
                LTarget::Other => {}
            }
        }
    }

    /// `(params) block end`; declares the parameters (and `self` for methods).
    fn func_body(
        &mut self,
        method: Option<(Value, String, Token)>,
        doc: Option<DocBlock>,
    ) -> Vec<String> {
        let mut params = Vec::new();
        let mut tokens = Vec::new();
        if self.eat("(") {
            loop {
                if self.eat("...") {
                    params.push("...".to_string());
                } else if let Some(n) = self.name() {
                    params.push(self.text(n).to_string());
                    tokens.push(n);
                } else {
                    break;
                }
                if !self.eat(",") {
                    break;
                }
            }
            self.eat(")");
        }
        self.fn_depth += 1;
        self.push_scope();
        if let Some((owner, container, name)) = method {
            self.decl(
                "self",
                DeclKind::Param,
                name.start,
                name.end,
                owner,
                None,
                container,
                None,
            );
        }
        for n in tokens {
            let name = self.text(n);
            let ty = doc
                .as_ref()
                .and_then(|d| d.param(name))
                .map(|p| p.ty.clone());
            let value = ty.map(Value::Typed).unwrap_or(Value::Unknown);
            self.decl(
                name,
                DeclKind::Param,
                n.start,
                n.end,
                value,
                None,
                String::new(),
                None,
            );
        }
        self.block();
        let end = self.here();
        self.pop_scope(end);
        self.eat("end");
        self.fn_depth -= 1;
        params
    }

    // ---- Expressions ----------------------------------------------------------------

    fn expr_list(&mut self) -> Vec<Expr> {
        let mut out = vec![self.expr()];
        while self.eat(",") {
            out.push(self.expr());
        }
        out
    }

    fn binary_op(&self) -> Option<&'a str> {
        let t = self.peek()?;
        let s = self.text(t);
        let is_op = match t.kind {
            Tok::Op => matches!(
                s,
                "+" | "-"
                    | "*"
                    | "/"
                    | "//"
                    | "%"
                    | "^"
                    | ".."
                    | "=="
                    | "~="
                    | "<"
                    | "<="
                    | ">"
                    | ">="
                    | "&"
                    | "|"
                    | "~"
                    | "<<"
                    | ">>"
            ),
            Tok::Keyword => matches!(s, "and" | "or"),
            _ => false,
        };
        is_op.then_some(s)
    }

    fn expr(&mut self) -> Expr {
        let first = self.unary();
        let Some(_) = self.binary_op() else {
            return first;
        };
        let mut operands = vec![first];
        let mut ops = Vec::new();
        while let Some(op) = self.binary_op() {
            self.p += 1;
            ops.push(op);
            operands.push(self.unary());
        }
        let value = if ops.iter().all(|o| *o == "or" || *o == "and") {
            // `M = M or {}`: prefer a table, else the last known value.
            operands
                .iter()
                .find(|e| matches!(e.value, Value::Table(_)))
                .or_else(|| operands.iter().rev().find(|e| e.value != Value::Unknown))
                .map(|e| e.value.clone())
                .unwrap_or(Value::Unknown)
        } else if ops.contains(&"..") {
            Value::Literal("string")
        } else if ops
            .iter()
            .any(|o| matches!(*o, "==" | "~=" | "<" | "<=" | ">" | ">="))
        {
            Value::Literal("boolean")
        } else if operands.iter().all(|e| e.value == Value::Literal("number")) {
            Value::Literal("number")
        } else {
            // Arithmetic on tables may go through metamethods (`Vec2 + Vec2`).
            Value::Unknown
        };
        Expr::of(value)
    }

    fn unary(&mut self) -> Expr {
        let mut op = None;
        while let Some(t) = self.peek() {
            let s = self.text(t);
            if (t.kind == Tok::Op && matches!(s, "-" | "#" | "~"))
                || (t.kind == Tok::Keyword && s == "not")
            {
                op = Some(s);
                self.p += 1;
            } else {
                break;
            }
        }
        let e = self.simple();
        match op {
            Some("not") => Expr::of(Value::Literal("boolean")),
            Some("#") => Expr::of(Value::Literal("number")),
            Some(_) if e.value == Value::Literal("number") => e,
            Some(_) => Expr::of(Value::Unknown),
            None => e,
        }
    }

    fn simple(&mut self) -> Expr {
        let Some(t) = self.peek() else {
            return Expr::of(Value::Unknown);
        };
        match t.kind {
            Tok::Number => {
                self.p += 1;
                Expr::of(Value::Literal("number"))
            }
            Tok::Str => {
                self.p += 1;
                Expr {
                    value: Value::Literal("string"),
                    params: None,
                    string: Some((string_value(self.src, &t), t.start, t.end)),
                }
            }
            Tok::Keyword => match self.text(t) {
                "nil" => {
                    self.p += 1;
                    Expr::of(Value::Literal("nil"))
                }
                "true" | "false" => {
                    self.p += 1;
                    Expr::of(Value::Literal("boolean"))
                }
                "function" => {
                    self.p += 1;
                    let doc = self.fn_doc.take();
                    let params = self.func_body(None, doc);
                    Expr {
                        value: Value::Function,
                        params: Some(params),
                        string: None,
                    }
                }
                _ => Expr::of(Value::Unknown),
            },
            Tok::Op if self.text(t) == "..." => {
                self.p += 1;
                Expr::of(Value::Unknown)
            }
            Tok::Op if self.text(t) == "{" => self.constructor(),
            _ => Expr::of(self.suffixed().0),
        }
    }

    fn constructor(&mut self) -> Expr {
        let id = self.tables;
        self.tables += 1;
        self.p += 1; // `{`
        let outer_fn_doc = self.fn_doc.take();
        while !self.is("}") && self.peek().is_some() {
            let before = self.p;
            let doc = self.take_doc();
            let key = if self.is("[") {
                self.p += 1;
                let k = self.expr();
                self.eat("]");
                if self.eat("=") {
                    k.string
                } else {
                    None
                }
            } else if self.peek().is_some_and(|t| t.kind == Tok::Name)
                && self.peek_at(1).is_some_and(|t| self.text(t) == "=")
            {
                let n = self.name().expect("name");
                self.p += 1; // `=`
                Some((self.text(n).to_string(), n.start, n.end))
            } else {
                None
            };
            self.fn_doc = doc.clone();
            let value = self.expr();
            self.fn_doc = None;
            if let Some((name, start, end)) = key {
                let kind = if value.value == Value::Function {
                    DeclKind::FieldFunction
                } else {
                    DeclKind::Field
                };
                let idx = self.decl(
                    &name,
                    kind,
                    start,
                    end,
                    value.value,
                    Some(Value::Table(id)),
                    String::new(),
                    doc,
                );
                self.out.decls[idx].params = value.params.unwrap_or_default();
            }
            if !self.eat(",") && !self.eat(";") && self.p == before {
                self.p += 1;
            }
        }
        self.eat("}");
        self.fn_doc = outer_fn_doc;
        Expr::of(Value::Table(id))
    }

    fn call_args(&mut self) -> Vec<Expr> {
        let Some(t) = self.peek() else {
            return Vec::new();
        };
        if t.kind == Tok::Str {
            return vec![self.simple()];
        }
        if self.is("{") {
            return vec![self.constructor()];
        }
        self.p += 1; // `(`
        let args = if self.is(")") {
            Vec::new()
        } else {
            self.expr_list()
        };
        self.eat(")");
        args
    }

    fn is_call_start(&self) -> bool {
        self.peek().is_some_and(|t| {
            t.kind == Tok::Str || (t.kind == Tok::Op && matches!(self.text(t), "(" | "{"))
        })
    }

    /// A primary expression with its suffixes (`a.b[c]:d(e)`), recording references.
    fn suffixed(&mut self) -> (Value, LTarget) {
        let Some(t) = self.peek() else {
            return (Value::Unknown, LTarget::Other);
        };
        let start = t.start;
        let (mut value, mut target) = if t.kind == Tok::Name {
            self.p += 1;
            let name = self.text(t).to_string();
            let value = self.resolve(&name);
            self.reference(t.start, t.end, target_of(&value, &name));
            let local = match value {
                Value::Local(i) => Some(i),
                _ => None,
            };
            (
                value,
                LTarget::Name {
                    name,
                    start: t.start,
                    end: t.end,
                    local,
                },
            )
        } else if self.is("(") {
            self.p += 1;
            let e = self.expr();
            self.eat(")");
            (e.value, LTarget::Other)
        } else {
            return (Value::Unknown, LTarget::Other);
        };
        loop {
            if self.is(".") || self.is("[") {
                let field = if self.eat(".") {
                    self.name()
                        .map(|n| (self.text(n).to_string(), n.start, n.end))
                } else {
                    self.p += 1;
                    let k = self.expr();
                    self.eat("]");
                    k.string
                };
                let Some((name, s, e)) = field else {
                    value = Value::Unknown;
                    target = LTarget::Other;
                    continue;
                };
                self.reference(s, e, Target::Field(value.clone(), name.clone()));
                let container = self.container_text(start, self.container_end(s), &value);
                target = LTarget::Field {
                    owner: value.clone(),
                    container,
                    name: name.clone(),
                    start: s,
                    end: e,
                };
                value = Value::Field(Box::new(value), name);
            } else if self.is(":") {
                self.p += 1;
                let Some(n) = self.name() else { break };
                let name = self.text(n).to_string();
                self.reference(n.start, n.end, Target::Field(value.clone(), name.clone()));
                if self.is_call_start() {
                    self.call_args();
                }
                value = Value::Call(Box::new(Value::Field(Box::new(value), name)));
                target = LTarget::Other;
            } else if self.is_call_start() {
                let args = self.call_args();
                value =
                    match &value {
                        Value::Global(g) if g == "require" => {
                            match args.first().and_then(|a| a.string.clone()) {
                                Some((module, s, e)) => {
                                    self.out.requires.push(RequireRef {
                                        start: s,
                                        end: e,
                                        module: module.clone(),
                                    });
                                    Value::Require(module)
                                }
                                None => Value::Unknown,
                            }
                        }
                        Value::Global(g) if g == "setmetatable" => {
                            let mut it = args.into_iter();
                            let table = it.next().map(|a| a.value).unwrap_or(Value::Unknown);
                            // `local Player = setmetatable({}, Entity)` declares a table;
                            // inside a constructor the result is an instance of the metatable.
                            match it.next().map(|a| a.value) {
                                Some(
                                    mt @ (Value::Local(_) | Value::Global(_) | Value::Field(..)),
                                ) if self.fn_depth > 0 || !matches!(table, Value::Table(_)) => mt,
                                _ => table,
                            }
                        }
                        _ => Value::Call(Box::new(value)),
                    };
                target = LTarget::Other;
            } else {
                break;
            }
        }
        (value, target)
    }

    /// End of the text before a `.name` / `[...]` suffix starting at `s`.
    fn container_end(&self, s: usize) -> usize {
        let before = &self.src[..s];
        let trimmed = before.trim_end();
        let trimmed = trimmed
            .strip_suffix('.')
            .or_else(|| trimmed.strip_suffix('[').map(str::trim_end))
            .unwrap_or(trimmed);
        // `t["x"]`: drop the bracket and quote.
        let trimmed = trimmed.trim_end_matches(['"', '\'']).trim_end_matches('[');
        trimmed.trim_end().len()
    }

    // ---- Post-processing ------------------------------------------------------------

    fn finish(&mut self) {
        // Name constructor fields after the variable holding the table.
        let mut table_names: HashMap<usize, String> = HashMap::new();
        for d in &self.out.decls {
            if let Value::Table(id) = d.value {
                let name = if d.kind.is_field() && !d.container.is_empty() {
                    format!("{}.{}", d.container, d.name)
                } else {
                    d.name.clone()
                };
                table_names.entry(id).or_insert(name);
            }
        }
        // Nested tables are named after their parents. Inner constructors are parsed
        // before the outer ones, so repeat until the names settle.
        for _ in 0..8 {
            let mut changed = false;
            for i in 0..self.out.decls.len() {
                if let Some(Value::Table(id)) = self.out.decls[i].owner {
                    if let Some(name) = table_names.get(&id).cloned() {
                        if self.out.decls[i].container != name {
                            self.out.decls[i].container = name.clone();
                            changed = true;
                        }
                        if let Value::Table(inner) = self.out.decls[i].value {
                            table_names.insert(inner, format!("{name}.{}", self.out.decls[i].name));
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }
}

/// The reference target of a resolved name.
fn target_of(value: &Value, name: &str) -> Target {
    match value {
        Value::Local(i) => Target::Local(*i),
        _ => Target::Global(name.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(f: &FileIndex) -> Vec<(String, DeclKind, String)> {
        f.decls
            .iter()
            .filter(|d| d.kind != DeclKind::Param && d.kind != DeclKind::ForVar)
            .map(|d| (d.name.clone(), d.kind, d.container.clone()))
            .collect()
    }

    #[test]
    fn functions_of_all_shapes() {
        let src = r#"
local M = {}

function M.new(x, y) return setmetatable({ x = x, y = y }, M) end
function M:len() return self.x end
M.scale = function(v, s) end
local function helper(a) end
function globalFn() end
local cb = function() end
Config = { debug = true, draw = function(dt) end }
return M
"#;
        let f = parse("vec.lua", src);
        assert_eq!(
            names(&f),
            vec![
                ("M".into(), DeclKind::Local, "".into()),
                ("new".into(), DeclKind::FieldFunction, "M".into()),
                ("x".into(), DeclKind::Field, "".into()),
                ("y".into(), DeclKind::Field, "".into()),
                ("len".into(), DeclKind::Method, "M".into()),
                ("scale".into(), DeclKind::FieldFunction, "M".into()),
                ("helper".into(), DeclKind::LocalFunction, "".into()),
                ("globalFn".into(), DeclKind::GlobalFunction, "".into()),
                ("cb".into(), DeclKind::LocalFunction, "".into()),
                ("debug".into(), DeclKind::Field, "Config".into()),
                ("draw".into(), DeclKind::FieldFunction, "Config".into()),
                ("Config".into(), DeclKind::Global, "".into()),
            ]
        );
        let new = &f.decls[1];
        assert_eq!(new.params, vec!["x", "y"]);
        assert_eq!(
            f.decls.iter().find(|d| d.name == "scale").unwrap().params,
            vec!["v", "s"]
        );
        assert_eq!(f.returns, Some(Value::Local(0)));
        assert_eq!(new.pos.line, 3);
    }

    #[test]
    fn locals_are_scoped_and_shadowed() {
        let src = "local x = 1\nlocal function f(x)\n  return x\nend\nprint(x)\nfor i, v in ipairs(t) do print(i) end";
        let f = parse("a.lua", src);
        let refs: Vec<(&str, &Target)> = f
            .refs
            .iter()
            .map(|r| (&src[r.start..r.end], &r.target))
            .collect();
        // `return x` inside f refers to the parameter, `print(x)` to the outer local.
        let param = f
            .decls
            .iter()
            .position(|d| d.kind == DeclKind::Param)
            .unwrap();
        assert!(refs.contains(&("x", &Target::Local(param))));
        assert!(refs.contains(&("x", &Target::Local(0))));
        assert!(refs.contains(&("print", &Target::Global("print".into()))));
        let i = f.decls.iter().position(|d| d.name == "i").unwrap();
        assert!(refs.contains(&("i", &Target::Local(i))));
        // The parameter is only visible inside the function.
        let p = &f.decls[param];
        assert!(p.scope.0 < src.find("return").unwrap());
        assert!(p.scope.1 <= src.find("print(x)").unwrap());
    }

    #[test]
    fn requires_and_fields_through_requires() {
        let src = "local vec = require(\"lib.vector\")\nlocal json = require 'json'\nlocal v = vec.new(1, 2)\nv:len()";
        let f = parse("main.lua", src);
        assert_eq!(
            f.requires
                .iter()
                .map(|r| r.module.as_str())
                .collect::<Vec<_>>(),
            vec!["lib.vector", "json"]
        );
        assert_eq!(
            &src[f.requires[0].start..f.requires[0].end],
            "\"lib.vector\""
        );
        assert_eq!(f.decls[0].value, Value::Require("lib.vector".into()));
        let new_ref = f
            .refs
            .iter()
            .find(|r| &src[r.start..r.end] == "new")
            .unwrap();
        assert_eq!(new_ref.target, Target::Field(Value::Local(0), "new".into()));
        assert_eq!(
            f.decls[2].value,
            Value::Call(Box::new(Value::Field(
                Box::new(Value::Local(0)),
                "new".into()
            )))
        );
    }

    #[test]
    fn annotations_attach_to_statements() {
        let src = r#"---@class Player : Entity
---@field name string
local Player = {}

--- Creates a player.
---@param name string The name
---@return Player
function Player.new(name) end

---@type number
local speed = 3

---@alias Dir "up"|"down"

--- Standalone, not attached.

local x = 1
"#;
        let f = parse("p.lua", src);
        assert_eq!(f.classes.len(), 1);
        assert_eq!(f.classes[0].decl, Some(0));
        assert_eq!(f.decls[0].class.as_deref(), Some("Player"));
        let new = f.decls.iter().find(|d| d.name == "new").unwrap();
        let doc = new.doc.as_ref().unwrap();
        assert_eq!(doc.description, "Creates a player.");
        assert_eq!(doc.returns[0].ty, "Player");
        let param = f
            .decls
            .iter()
            .find(|d| d.name == "name" && d.kind == DeclKind::Param)
            .unwrap();
        assert_eq!(param.value, Value::Typed("string".into()));
        assert_eq!(
            f.decls
                .iter()
                .find(|d| d.name == "speed")
                .unwrap()
                .annotated_type(),
            Some("number")
        );
        assert_eq!(f.aliases[0].ann.name, "Dir");
        assert!(f
            .decls
            .iter()
            .find(|d| d.name == "x")
            .unwrap()
            .doc
            .is_none());
    }

    #[test]
    fn self_fields_belong_to_the_method_table() {
        let src = "local P = {}\nfunction P:init() self.hp = 10 end";
        let f = parse("p.lua", src);
        let hp = f.decls.iter().find(|d| d.name == "hp").unwrap();
        assert_eq!(hp.kind, DeclKind::Field);
        assert_eq!(hp.container, "P");
        let this = f.decls.iter().find(|d| d.name == "self").unwrap();
        assert_eq!(
            hp.owner,
            Some(Value::Local(
                f.decls.iter().position(|d| std::ptr::eq(d, this)).unwrap()
            ))
        );
        assert_eq!(this.value, Value::Local(0));
    }

    #[test]
    fn tolerates_broken_code() {
        let src = "local function f(\n  if x then\nend end end\nlocal y = {\nfunction M.g() end";
        let f = parse("broken.lua", src);
        assert!(f.decls.iter().any(|d| d.name == "f"));
        // Every byte offset still maps to something sensible.
        let _ = f.ref_at(5);
    }

    #[test]
    fn nested_constructor_fields_are_named_after_their_path() {
        let src = "local conf = { window = { width = 800 } }";
        let f = parse("conf.lua", src);
        let width = f.decls.iter().find(|d| d.name == "width").unwrap();
        assert_eq!(width.container, "conf.window");
    }
}
