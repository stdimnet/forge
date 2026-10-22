//! The document tree both parsers produce: JSON values with the byte ranges they came from,
//! so problems, completion and hover map back to the source.

/// A value with its byte range in the source (quotes and brackets included).
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub value: Value,
    pub start: usize,
    pub end: usize,
    /// A YAML tag other than the core ones (`!reference`, `!reset`): its meaning is up to
    /// the tool reading the file, so the schema does not judge the value.
    pub custom_tag: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Node>),
    Object(Vec<Property>),
    /// Something that is not a value (a typo, an unfinished literal). Never validated.
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub key: String,
    pub key_start: usize,
    pub key_end: usize,
    /// Byte offset of the `:`; `None` while the key is being typed.
    pub colon: Option<usize>,
    pub value: Option<Node>,
}

impl Node {
    pub fn new(value: Value, start: usize, end: usize) -> Node {
        Node {
            value,
            start,
            end,
            custom_tag: false,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Node> {
        match &self.value {
            Value::Object(props) => props
                .iter()
                .find(|p| p.key == key)
                .and_then(|p| p.value.as_ref()),
            _ => None,
        }
    }

    pub fn property(&self, key: &str) -> Option<&Property> {
        match &self.value {
            Value::Object(props) => props.iter().find(|p| p.key == key),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match &self.value {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self.value {
            Value::Number(n) => Some(n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self.value {
            Value::Bool(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Node]> {
        match &self.value {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&[Property]> {
        match &self.value {
            Value::Object(props) => Some(props),
            _ => None,
        }
    }

    /// The JSON Schema type name: `integer` for numbers without a fraction.
    pub fn type_name(&self) -> &'static str {
        match &self.value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(n) if n.fract() == 0.0 && n.is_finite() => "integer",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
            Value::Error => "error",
        }
    }

    /// Structural equality of the values, ignoring where they are written.
    pub fn same_value(&self, other: &Node) -> bool {
        match (&self.value, &other.value) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.same_value(y))
            }
            (Value::Object(a), Value::Object(b)) => {
                let a: Vec<_> = a.iter().filter(|p| p.value.is_some()).collect();
                let b: Vec<_> = b.iter().filter(|p| p.value.is_some()).collect();
                a.len() == b.len()
                    && a.iter().all(|p| {
                        b.iter().any(|q| {
                            q.key == p.key
                                && q.value
                                    .as_ref()
                                    .zip(p.value.as_ref())
                                    .is_some_and(|(x, y)| x.same_value(y))
                        })
                    })
            }
            _ => false,
        }
    }

    /// The value as compact JSON text (for messages and completion items).
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out);
        out
    }

    fn write_json(&self, out: &mut String) {
        match &self.value {
            Value::Null | Value::Error => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Number(n) => out.push_str(&format_number(*n)),
            Value::String(s) => out.push_str(&quote(s)),
            Value::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    item.write_json(out);
                }
                out.push(']');
            }
            Value::Object(props) => {
                out.push('{');
                let mut first = true;
                for p in props {
                    let Some(v) = &p.value else { continue };
                    if !first {
                        out.push_str(", ");
                    }
                    first = false;
                    out.push_str(&quote(&p.key));
                    out.push_str(": ");
                    v.write_json(out);
                }
                out.push('}');
            }
        }
    }

    /// The deepest node (with the property it is the value of) whose range contains `offset`.
    pub fn node_at(&self, offset: usize) -> Option<&Node> {
        if offset < self.start || offset > self.end {
            return None;
        }
        match &self.value {
            Value::Array(items) => {
                for item in items {
                    if let Some(n) = item.node_at(offset) {
                        return Some(n);
                    }
                }
            }
            Value::Object(props) => {
                for p in props {
                    if let Some(n) = p.value.as_ref().and_then(|v| v.node_at(offset)) {
                        return Some(n);
                    }
                }
            }
            _ => {}
        }
        Some(self)
    }

    /// Move the whole subtree to `start..end` (merged YAML keys report at the `<<` key).
    pub fn relocate(&mut self, start: usize, end: usize) {
        self.start = start;
        self.end = end;
        match &mut self.value {
            Value::Array(items) => items.iter_mut().for_each(|i| i.relocate(start, end)),
            Value::Object(props) => {
                for p in props {
                    p.key_start = start;
                    p.key_end = end;
                    p.colon = p.colon.map(|_| end);
                    if let Some(v) = &mut p.value {
                        v.relocate(start, end);
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.is_finite() && n.abs() < 1e16 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// A JSON string literal.
pub fn quote(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\""))
}

/// A segment of a path into a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seg {
    Key(String),
    Index(usize),
}
