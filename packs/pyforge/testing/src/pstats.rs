//! cProfile results (`python -m cProfile -o run.pstats`): the `.pstats` file is a Python
//! `marshal` dump of `{(file, line, function): (primitive calls, calls, own time, total
//! time, {caller: (calls, primitive calls, own time, total time)})}`. This module reads the
//! marshal subset those files use and builds what the Profiler tool window shows: hot spots
//! (own and total time, calls) and the call tree (callers expanded into callees).

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

/// A value of the marshal format.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Bytes(Vec<u8>),
    Tuple(Vec<Value>),
    List(Vec<Value>),
    Dict(Vec<(Value, Value)>),
    Set(Vec<Value>),
    Other,
}

impl Value {
    fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            Value::Bool(b) => Some(f64::from(u8::from(*b))),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    fn items(&self) -> &[Value] {
        match self {
            Value::Tuple(v) | Value::List(v) | Value::Set(v) => v,
            _ => &[],
        }
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    refs: Vec<Value>,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(n).filter(|e| *e <= self.data.len());
        let end = end.ok_or_else(|| "the profile is truncated".to_string())?;
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn i32(&mut self) -> Result<i32, String> {
        let b = self.take(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn len(&mut self) -> Result<usize, String> {
        let n = self.i32()?;
        usize::try_from(n).map_err(|_| "negative length in the profile".to_string())
    }

    fn string(&mut self, n: usize) -> Result<String, String> {
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }

    fn value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > 64 {
            return Err("the profile nests too deeply".into());
        }
        let code = self.u8()?;
        let flag = code & 0x80 != 0;
        let slot = flag.then(|| {
            self.refs.push(Value::None);
            self.refs.len() - 1
        });
        let v = match code & 0x7f {
            b'0' => return Err("unexpected end marker".into()),
            b'N' => Value::None,
            b'F' => Value::Bool(false),
            b'T' => Value::Bool(true),
            b'S' | b'.' => Value::Other,
            b'i' => Value::Int(i64::from(self.i32()?)),
            b'I' => {
                let b = self.take(8)?;
                let mut a = [0u8; 8];
                a.copy_from_slice(b);
                Value::Int(i64::from_le_bytes(a))
            }
            b'l' => {
                let n = self.i32()?;
                let mut value: f64 = 0.0;
                let digits = n.unsigned_abs() as usize;
                let mut scale = 1.0;
                for _ in 0..digits {
                    let b = self.take(2)?;
                    value += f64::from(u16::from_le_bytes([b[0], b[1]])) * scale;
                    scale *= 32768.0;
                }
                let v = if n < 0 { -value } else { value };
                if v.abs() < 9.0e15 {
                    Value::Int(v as i64)
                } else {
                    Value::Float(v)
                }
            }
            b'g' => {
                let b = self.take(8)?;
                let mut a = [0u8; 8];
                a.copy_from_slice(b);
                Value::Float(f64::from_le_bytes(a))
            }
            b'f' => {
                let n = self.u8()? as usize;
                let s = self.string(n)?;
                Value::Float(s.trim().parse().unwrap_or(0.0))
            }
            b'y' => {
                self.take(16)?;
                Value::Other
            }
            b'x' => {
                let n = self.u8()? as usize;
                self.take(n)?;
                let n = self.u8()? as usize;
                self.take(n)?;
                Value::Other
            }
            b's' => {
                let n = self.len()?;
                Value::Bytes(self.take(n)?.to_vec())
            }
            b't' | b'u' | b'a' | b'A' => {
                let n = self.len()?;
                Value::Str(self.string(n)?)
            }
            b'z' | b'Z' => {
                let n = self.u8()? as usize;
                Value::Str(self.string(n)?)
            }
            b'(' | b'[' | b'<' | b'>' => {
                let n = self.len()?;
                if n > self.data.len() {
                    return Err("bad container size in the profile".into());
                }
                let mut items = Vec::with_capacity(n);
                for _ in 0..n {
                    items.push(self.value(depth + 1)?);
                }
                match code & 0x7f {
                    b'(' => Value::Tuple(items),
                    b'[' => Value::List(items),
                    _ => Value::Set(items),
                }
            }
            b')' => {
                let n = self.u8()? as usize;
                let mut items = Vec::with_capacity(n);
                for _ in 0..n {
                    items.push(self.value(depth + 1)?);
                }
                Value::Tuple(items)
            }
            b'{' => {
                let mut pairs = Vec::new();
                loop {
                    if self.data.get(self.pos) == Some(&b'0') {
                        self.pos += 1;
                        break;
                    }
                    let k = self.value(depth + 1)?;
                    let v = self.value(depth + 1)?;
                    pairs.push((k, v));
                }
                Value::Dict(pairs)
            }
            b'r' => {
                let i = self.len()?;
                self.refs
                    .get(i)
                    .cloned()
                    .ok_or_else(|| "bad reference in the profile".to_string())?
            }
            other => {
                return Err(format!(
                    "unsupported marshal type '{}' in the profile",
                    other as char
                ))
            }
        };
        if let Some(s) = slot {
            self.refs[s] = v.clone();
        }
        Ok(v)
    }
}

/// Read a marshal dump.
pub fn unmarshal(data: &[u8]) -> Result<Value, String> {
    let mut r = Reader {
        data,
        pos: 0,
        refs: Vec::new(),
    };
    r.value(0)
}

/// A function of the profile (`file`, 1-based `line`; built-ins have the file `~`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Function {
    pub id: usize,
    pub file: String,
    pub line: u32,
    pub name: String,
    /// Calls, and the primitive (not recursive) ones.
    pub calls: u64,
    pub primitive_calls: u64,
    /// Seconds spent in the function itself and including what it called.
    pub own_time: f64,
    pub total_time: f64,
    /// Callers: (function id, calls, own time, total time) through that caller.
    pub callers: Vec<Edge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub function: usize,
    pub calls: u64,
    pub own_time: f64,
    pub total_time: f64,
}

impl Function {
    /// Built-ins and C functions (no source file).
    pub fn is_builtin(&self) -> bool {
        self.file == "~" || self.file.is_empty()
    }

    /// `main.py:12(load_orders)`, `{built-in method time.sleep}` as pstats prints them.
    pub fn label(&self) -> String {
        if self.is_builtin() {
            if self.name.starts_with('<') {
                format!(
                    "{{{}}}",
                    self.name.trim_start_matches('<').trim_end_matches('>')
                )
            } else {
                self.name.clone()
            }
        } else {
            self.name.clone()
        }
    }
}

/// A whole profile.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub functions: Vec<Function>,
    /// Seconds: the sum of every function's own time.
    pub total_time: f64,
}

fn function_key(v: &Value) -> Option<(String, u32, String)> {
    let items = v.items();
    Some((
        items.first()?.as_str()?.to_string(),
        items.get(1)?.as_f64()? as u32,
        items.get(2)?.as_str()?.to_string(),
    ))
}

/// Read a `.pstats` file.
pub fn parse(data: &[u8]) -> Result<Profile, String> {
    let Value::Dict(entries) = unmarshal(data)? else {
        return Err("not a cProfile result (.pstats)".into());
    };
    let mut ids: HashMap<(String, u32, String), usize> = HashMap::new();
    let mut functions: Vec<Function> = Vec::new();
    let mut id_of = |key: (String, u32, String), functions: &mut Vec<Function>| -> usize {
        *ids.entry(key.clone()).or_insert_with(|| {
            functions.push(Function {
                id: functions.len(),
                file: key.0,
                line: key.1,
                name: key.2,
                calls: 0,
                primitive_calls: 0,
                own_time: 0.0,
                total_time: 0.0,
                callers: Vec::new(),
            });
            functions.len() - 1
        })
    };
    for (k, v) in &entries {
        let Some(key) = function_key(k) else {
            continue;
        };
        let stats = v.items();
        let num = |i: usize| stats.get(i).and_then(Value::as_f64).unwrap_or(0.0);
        let id = id_of(key, &mut functions);
        functions[id].primitive_calls = num(0) as u64;
        functions[id].calls = num(1) as u64;
        functions[id].own_time = num(2);
        functions[id].total_time = num(3);
        let mut callers = Vec::new();
        if let Some(Value::Dict(cs)) = stats.get(4) {
            for (ck, cv) in cs {
                let Some(caller) = function_key(ck) else {
                    continue;
                };
                let cid = id_of(caller, &mut functions);
                // cProfile: (calls, primitive calls, own, total); old profile: a count.
                let e = cv.items();
                let n = |i: usize| e.get(i).and_then(Value::as_f64).unwrap_or(0.0);
                callers.push(Edge {
                    function: cid,
                    calls: if e.is_empty() {
                        cv.as_f64().unwrap_or(0.0) as u64
                    } else {
                        n(0) as u64
                    },
                    own_time: n(2),
                    total_time: n(3),
                });
            }
        }
        functions[id].callers = callers;
    }
    let total_time = functions.iter().map(|f| f.own_time).sum();
    Ok(Profile {
        functions,
        total_time,
    })
}

/// A row of the hot spots table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotSpot {
    pub function: usize,
    pub name: String,
    pub file: String,
    pub line: u32,
    pub calls: u64,
    pub primitive_calls: u64,
    /// Milliseconds.
    pub own_ms: f64,
    pub total_ms: f64,
    /// Share of the profile's time, 0–100.
    pub own_percent: f64,
    pub total_percent: f64,
}

/// A node of the call tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallNode {
    pub function: usize,
    pub name: String,
    pub file: String,
    pub line: u32,
    /// Calls from the parent (all calls for roots).
    pub calls: u64,
    pub own_ms: f64,
    pub total_ms: f64,
    pub total_percent: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<CallNode>,
}

/// What the Profiler tool window shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileView {
    pub total_ms: f64,
    pub functions: usize,
    pub calls: u64,
    pub hot_spots: Vec<HotSpot>,
    pub call_tree: Vec<CallNode>,
}

/// Limits of the call tree: deeper or cheaper calls are left out.
#[derive(Debug, Clone, Copy)]
pub struct TreeLimits {
    pub max_depth: usize,
    pub max_nodes: usize,
    /// Calls below this share (0–100) of the total time are left out below the first
    /// level.
    pub min_percent: f64,
}

impl Default for TreeLimits {
    fn default() -> Self {
        Self {
            max_depth: 48,
            max_nodes: 4000,
            min_percent: 0.2,
        }
    }
}

fn percent(part: f64, total: f64) -> f64 {
    if total > 0.0 {
        (part / total * 1000.0).round() / 10.0
    } else {
        0.0
    }
}

fn ms(seconds: f64) -> f64 {
    (seconds * 100_000.0).round() / 100.0
}

impl Profile {
    /// Functions by own time, the most expensive first.
    pub fn hot_spots(&self, limit: usize) -> Vec<HotSpot> {
        let mut fs: Vec<&Function> = self.functions.iter().collect();
        fs.sort_by(|a, b| {
            b.own_time
                .total_cmp(&a.own_time)
                .then(b.total_time.total_cmp(&a.total_time))
        });
        fs.into_iter()
            .take(limit)
            .map(|f| HotSpot {
                function: f.id,
                name: f.label(),
                file: f.file.clone(),
                line: f.line,
                calls: f.calls,
                primitive_calls: f.primitive_calls,
                own_ms: ms(f.own_time),
                total_ms: ms(f.total_time),
                own_percent: percent(f.own_time, self.total_time),
                total_percent: percent(f.total_time, self.total_time),
            })
            .collect()
    }

    /// Callees of every function: (callee, the edge's stats).
    fn callees(&self) -> Vec<Vec<(usize, &Edge)>> {
        let mut out: Vec<Vec<(usize, &Edge)>> = vec![Vec::new(); self.functions.len()];
        for f in &self.functions {
            for e in &f.callers {
                if let Some(list) = out.get_mut(e.function) {
                    list.push((f.id, e));
                }
            }
        }
        for list in &mut out {
            list.sort_by(|a, b| b.1.total_time.total_cmp(&a.1.total_time));
        }
        out
    }

    /// The call tree from the functions nobody calls (the profiled program's entry).
    pub fn call_tree(&self, limits: TreeLimits) -> Vec<CallNode> {
        let callees = self.callees();
        let mut roots: Vec<&Function> = self
            .functions
            .iter()
            .filter(|f| f.callers.iter().all(|c| c.function == f.id))
            .collect();
        roots.sort_by(|a, b| b.total_time.total_cmp(&a.total_time));
        let mut budget = limits.max_nodes;
        let mut path: Vec<usize> = Vec::new();
        roots
            .into_iter()
            .filter_map(|f| {
                if budget == 0 {
                    return None;
                }
                budget -= 1;
                path.push(f.id);
                let children = self.children(f.id, &callees, &limits, &mut path, &mut budget);
                path.pop();
                Some(self.node(f, f.calls, f.own_time, f.total_time, children))
            })
            .collect()
    }

    fn node(
        &self,
        f: &Function,
        calls: u64,
        own: f64,
        total: f64,
        children: Vec<CallNode>,
    ) -> CallNode {
        CallNode {
            function: f.id,
            name: f.label(),
            file: f.file.clone(),
            line: f.line,
            calls,
            own_ms: ms(own),
            total_ms: ms(total),
            total_percent: percent(total, self.total_time),
            children,
        }
    }

    fn children(
        &self,
        id: usize,
        callees: &[Vec<(usize, &Edge)>],
        limits: &TreeLimits,
        path: &mut Vec<usize>,
        budget: &mut usize,
    ) -> Vec<CallNode> {
        if path.len() >= limits.max_depth {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (callee, edge) in &callees[id] {
            if *budget == 0 {
                break;
            }
            // Recursion shows once.
            if path.contains(callee) {
                continue;
            }
            if path.len() > 1 && percent(edge.total_time, self.total_time) < limits.min_percent {
                continue;
            }
            *budget -= 1;
            path.push(*callee);
            let children = self.children(*callee, callees, limits, path, budget);
            path.pop();
            out.push(self.node(
                &self.functions[*callee],
                edge.calls,
                edge.own_time,
                edge.total_time,
                children,
            ));
        }
        out
    }

    pub fn view(&self, hot_spots: usize, limits: TreeLimits) -> ProfileView {
        ProfileView {
            total_ms: ms(self.total_time),
            functions: self.functions.len(),
            calls: self.functions.iter().map(|f| f.calls).sum(),
            hot_spots: self.hot_spots(hot_spots),
            call_tree: self.call_tree(limits),
        }
    }

    /// Rewrite the functions' files (container paths to project paths).
    pub fn map_files(&mut self, map: impl Fn(&str) -> Option<String>) {
        let mut cache: BTreeMap<String, Option<String>> = BTreeMap::new();
        for f in &mut self.functions {
            if f.is_builtin() {
                continue;
            }
            let mapped = cache
                .entry(f.file.clone())
                .or_insert_with(|| map(&f.file))
                .clone();
            if let Some(m) = mapped {
                f.file = m;
            }
        }
    }
}

/// Writes marshal data (tests and the browser demo's profiles).
pub mod write {
    use super::Value;

    pub fn value(out: &mut Vec<u8>, v: &Value) {
        match v {
            Value::None => out.push(b'N'),
            Value::Bool(b) => out.push(if *b { b'T' } else { b'F' }),
            Value::Int(i) => {
                out.push(b'i');
                out.extend((*i as i32).to_le_bytes());
            }
            Value::Float(f) => {
                out.push(b'g');
                out.extend(f.to_le_bytes());
            }
            Value::Str(s) => {
                out.push(b'u');
                out.extend((s.len() as i32).to_le_bytes());
                out.extend(s.as_bytes());
            }
            Value::Bytes(b) => {
                out.push(b's');
                out.extend((b.len() as i32).to_le_bytes());
                out.extend(b);
            }
            Value::Tuple(items) | Value::List(items) | Value::Set(items) => {
                out.push(b'(');
                out.extend((items.len() as i32).to_le_bytes());
                for i in items {
                    value(out, i);
                }
            }
            Value::Dict(pairs) => {
                out.push(b'{');
                for (k, v) in pairs {
                    value(out, k);
                    value(out, v);
                }
                out.push(b'0');
            }
            Value::Other => out.push(b'N'),
        }
    }

    /// A `.pstats` file from (file, line, name, calls, own s, total s, callers: (index,
    /// calls, own s, total s)).
    pub type Row<'a> = (
        &'a str,
        u32,
        &'a str,
        u64,
        f64,
        f64,
        Vec<(usize, u64, f64, f64)>,
    );

    pub fn pstats(rows: &[Row]) -> Vec<u8> {
        let key = |r: &Row| {
            Value::Tuple(vec![
                Value::Str(r.0.to_string()),
                Value::Int(i64::from(r.1)),
                Value::Str(r.2.to_string()),
            ])
        };
        let dict = rows
            .iter()
            .map(|r| {
                let callers =
                    r.6.iter()
                        .map(|(i, n, own, total)| {
                            (
                                key(&rows[*i]),
                                Value::Tuple(vec![
                                    Value::Int(*n as i64),
                                    Value::Int(*n as i64),
                                    Value::Float(*own),
                                    Value::Float(*total),
                                ]),
                            )
                        })
                        .collect();
                (
                    key(r),
                    Value::Tuple(vec![
                        Value::Int(r.3 as i64),
                        Value::Int(r.3 as i64),
                        Value::Float(r.4),
                        Value::Float(r.5),
                        Value::Dict(callers),
                    ]),
                )
            })
            .collect();
        let mut out = Vec::new();
        value(&mut out, &Value::Dict(dict));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `python -c "import cProfile; cProfile.run('sum(range(10))', 'x.pstats')"` on
    /// CPython 3.13, as bytes (interned short strings, references, binary floats).
    fn fixture() -> Vec<u8> {
        let path = format!("{}/fixtures/sum.pstats", env!("CARGO_MANIFEST_DIR"));
        std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    #[test]
    fn reads_a_real_cprofile_file() {
        let p = parse(&fixture()).unwrap();
        assert!(p
            .functions
            .iter()
            .any(|f| f.name.contains("built-in method builtins.sum")));
        let module = p
            .functions
            .iter()
            .find(|f| f.name == "<module>")
            .expect("module");
        assert_eq!(module.calls, 1);
        let tree = p.call_tree(TreeLimits::default());
        assert!(!tree.is_empty());
        let view = p.view(10, TreeLimits::default());
        assert!(view.functions >= 3);
        assert!(view.hot_spots[0].own_ms >= view.hot_spots.last().unwrap().own_ms);
    }

    #[test]
    fn builds_hot_spots_and_the_call_tree() {
        let rows: Vec<write::Row> = vec![
            (
                "~",
                0,
                "<built-in method builtins.exec>",
                1,
                0.0,
                1.0,
                vec![],
            ),
            (
                "/app/main.py",
                1,
                "<module>",
                1,
                0.01,
                1.0,
                vec![(0, 1, 0.01, 1.0)],
            ),
            (
                "/app/main.py",
                10,
                "load",
                2,
                0.6,
                0.7,
                vec![(1, 2, 0.6, 0.7)],
            ),
            (
                "/app/main.py",
                20,
                "parse",
                4,
                0.29,
                0.29,
                vec![(1, 1, 0.09, 0.09), (2, 3, 0.2, 0.2)],
            ),
            (
                "/app/main.py",
                30,
                "fact",
                3,
                0.001,
                0.001,
                vec![(3, 1, 0.001, 0.001), (4, 2, 0.0, 0.0)],
            ),
        ];
        let p = parse(&write::pstats(&rows)).unwrap();
        assert_eq!(p.functions.len(), 5);
        assert!((p.total_time - 0.901).abs() < 1e-9);
        let hot = p.hot_spots(3);
        assert_eq!(hot[0].name, "load");
        assert_eq!(hot[0].own_ms, 600.0);
        assert_eq!(hot[0].own_percent, 66.6);
        assert_eq!(hot[1].name, "parse");
        assert_eq!(hot[1].calls, 4);
        let tree = p.call_tree(TreeLimits::default());
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "{built-in method builtins.exec}");
        let module = &tree[0].children[0];
        assert_eq!(module.name, "<module>");
        assert_eq!(
            module
                .children
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            ["load", "parse"]
        );
        let load = &module.children[0];
        assert_eq!(load.calls, 2);
        assert_eq!(load.children[0].name, "parse");
        assert_eq!(load.children[0].calls, 3);
        // The recursive `fact` shows once; cheap calls deep down are left out.
        let fact_depth: Vec<_> = load.children[0]
            .children
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert!(fact_depth.is_empty() || fact_depth == ["fact"]);
        let mut mapped = p.clone();
        mapped.map_files(|f| f.strip_prefix("/app/").map(str::to_string));
        assert_eq!(mapped.functions[1].file, "main.py");
        assert_eq!(mapped.functions[0].file, "~");
    }

    #[test]
    fn rejects_other_files() {
        assert!(parse(b"").is_err());
        assert!(parse(b"N").is_err());
        assert!(parse(b"(\x01\x00\x00").is_err());
        assert!(parse(b"\x7f").is_err());
    }
}
