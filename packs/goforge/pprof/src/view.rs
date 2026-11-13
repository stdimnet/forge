//! The Profiler's views of a profile for one sample type: the top functions (flat and
//! cumulative, like `pprof -top`), the call tree (top-down, merged by function) and the
//! flame graph (the UI draws it from the same tree).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::profile::{Profile, ValueType};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopRow {
    pub name: String,
    pub file: String,
    /// The function's first line (1-based, 0: unknown).
    pub line: u32,
    /// Value in the function itself.
    pub flat: i64,
    /// Value in the function and everything it called.
    pub cum: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallNode {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub total: i64,
    #[serde(rename = "self")]
    pub self_value: i64,
    pub children: Vec<CallNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileView {
    pub sample_types: Vec<ValueType>,
    pub sample_index: usize,
    /// Sum of the sample type over all samples.
    pub total: i64,
    pub samples: usize,
    pub duration_nanos: i64,
    pub period_type: Option<ValueType>,
    pub period: i64,
    pub top: Vec<TopRow>,
    /// The root (`name` "root") with the call tree below it.
    pub tree: CallNode,
    /// Nodes below `minFraction` of the total were left out.
    pub pruned: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ViewOptions {
    /// Rows of the top list.
    pub top_limit: usize,
    /// Call tree nodes smaller than this fraction of the total are left out.
    pub min_fraction: f64,
    /// Deepest call tree level kept.
    pub max_depth: usize,
}

impl Default for ViewOptions {
    fn default() -> Self {
        ViewOptions {
            top_limit: 200,
            min_fraction: 0.002,
            max_depth: 80,
        }
    }
}

struct Node {
    func: usize,
    total: i64,
    self_value: i64,
    children: HashMap<usize, usize>,
}

/// The views of `profile` for the sample type `sample_index` (default: the profile's).
pub fn view(profile: &Profile, sample_index: Option<usize>, options: ViewOptions) -> ProfileView {
    let idx = sample_index
        .filter(|i| *i < profile.sample_types.len())
        .unwrap_or(profile.default_sample_type);
    // Functions by name and file (frames differ by line).
    let mut func_index: HashMap<(&str, &str), usize> = HashMap::new();
    let mut funcs: Vec<(&str, &str, u32)> = Vec::new();
    let frame_func: Vec<usize> = profile
        .frames
        .iter()
        .map(|f| {
            *func_index
                .entry((f.function.as_str(), f.file.as_str()))
                .or_insert_with(|| {
                    funcs.push((&f.function, &f.file, f.start_line));
                    funcs.len() - 1
                })
        })
        .collect();
    let mut flat = vec![0i64; funcs.len()];
    let mut cum = vec![0i64; funcs.len()];
    let mut nodes = vec![Node {
        func: usize::MAX,
        total: 0,
        self_value: 0,
        children: HashMap::new(),
    }];
    let mut total = 0i64;
    let mut seen: Vec<usize> = Vec::new();
    for s in &profile.samples {
        let v = s.values.get(idx).copied().unwrap_or(0);
        if v == 0 {
            continue;
        }
        total += v;
        let stack: Vec<usize> = s.stack.iter().map(|f| frame_func[*f]).collect();
        if let Some(leaf) = stack.first() {
            flat[*leaf] += v;
        }
        seen.clear();
        for f in &stack {
            if !seen.contains(f) {
                seen.push(*f);
                cum[*f] += v;
            }
        }
        // Top-down: root first.
        let mut node = 0;
        nodes[0].total += v;
        for (depth, f) in stack.iter().rev().enumerate() {
            if depth >= options.max_depth {
                break;
            }
            let next = match nodes[node].children.get(f) {
                Some(n) => *n,
                None => {
                    nodes.push(Node {
                        func: *f,
                        total: 0,
                        self_value: 0,
                        children: HashMap::new(),
                    });
                    let n = nodes.len() - 1;
                    nodes[node].children.insert(*f, n);
                    n
                }
            };
            node = next;
            nodes[node].total += v;
        }
        nodes[node].self_value += v;
    }
    let mut top: Vec<TopRow> = funcs
        .iter()
        .enumerate()
        .filter(|(i, _)| cum[*i] != 0 || flat[*i] != 0)
        .map(|(i, (name, file, line))| TopRow {
            name: name.to_string(),
            file: file.to_string(),
            line: *line,
            flat: flat[i],
            cum: cum[i],
        })
        .collect();
    top.sort_by(|a, b| {
        b.flat
            .cmp(&a.flat)
            .then(b.cum.cmp(&a.cum))
            .then(a.name.cmp(&b.name))
    });
    top.truncate(options.top_limit);
    let min = (total.unsigned_abs() as f64 * options.min_fraction) as i64;
    let mut pruned = 0;
    let tree = build(&nodes, 0, &funcs, min, &mut pruned);
    ProfileView {
        sample_types: profile.sample_types.clone(),
        sample_index: idx,
        total,
        samples: profile.samples.len(),
        duration_nanos: profile.duration_nanos,
        period_type: profile.period_type.clone(),
        period: profile.period,
        top,
        tree,
        pruned,
    }
}

fn build(
    nodes: &[Node],
    i: usize,
    funcs: &[(&str, &str, u32)],
    min: i64,
    pruned: &mut usize,
) -> CallNode {
    let n = &nodes[i];
    let (name, file, line) = funcs
        .get(n.func)
        .map(|(n, f, l)| (n.to_string(), f.to_string(), *l))
        .unwrap_or_else(|| ("root".to_string(), String::new(), 0));
    let mut children: Vec<CallNode> = Vec::new();
    for c in n.children.values() {
        if nodes[*c].total.abs() < min {
            *pruned += 1;
            continue;
        }
        children.push(build(nodes, *c, funcs, min, pruned));
    }
    children.sort_by(|a, b| b.total.cmp(&a.total).then(a.name.cmp(&b.name)));
    CallNode {
        name,
        file,
        line,
        total: n.total,
        self_value: n.self_value,
        children,
    }
}
