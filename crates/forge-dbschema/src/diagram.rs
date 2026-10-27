//! ER diagrams ("Diagrams | Show Diagram", Ctrl+Alt+Shift+U): the tables of a schema, or
//! selected tables with the tables they reference and that reference them, laid out in
//! layers — a referenced table sits above the tables whose foreign keys point to it — with
//! each foreign key drawn from the row of its column to the row of the referenced column.
//!
//! The UI only draws what this module computes: every node has its position and size, every
//! column its row, and every edge its polyline. Text widths are estimated for a 12px
//! monospace font (`CHAR_WIDTH`), which the diagram view uses.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use forge_dbmodel::{SchemaInfo, TableInfo, TableKind};
use serde::{Deserialize, Serialize};

/// Height of a table's title bar.
pub const HEADER_HEIGHT: f64 = 28.0;
/// Height of one column row.
pub const ROW_HEIGHT: f64 = 20.0;
/// Estimated width of one character of the diagram font (12px monospace).
pub const CHAR_WIDTH: f64 = 7.3;
/// At most this many columns are listed per table; the rest is "… N more".
pub const MAX_COLUMNS: usize = 30;
/// Longer types are shortened in the box (`enum('pending','paid',…`).
pub const MAX_TYPE_CHARS: usize = 24;

const PADDING: f64 = 10.0;
const MIN_WIDTH: f64 = 150.0;
const MAX_WIDTH: f64 = 380.0;
const LAYER_GAP: f64 = 70.0;
const NODE_GAP: f64 = 50.0;
const COMPONENT_GAP: f64 = 90.0;
const MARGIN: f64 = 24.0;
const LANE: f64 = 22.0;

/// Which columns the tables list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ColumnsMode {
    #[default]
    All,
    /// Primary and foreign key columns only.
    Keys,
    /// Table names only.
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiagramRequest {
    pub schema: SchemaInfo,
    /// Tables to show; empty shows the whole schema.
    #[serde(default)]
    pub tables: Vec<String>,
    /// Also show the tables the selected ones reference and that reference them.
    #[serde(default)]
    pub neighbours: bool,
    #[serde(default)]
    pub columns: ColumnsMode,
    /// Leave views and materialized views out.
    #[serde(default)]
    pub hide_views: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiagramColumn {
    pub name: String,
    pub data_type: String,
    /// The type as the box shows it (shortened to [`MAX_TYPE_CHARS`]).
    #[serde(default)]
    pub type_label: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub foreign: bool,
    #[serde(default)]
    pub nullable: bool,
    /// Part of a unique index.
    #[serde(default)]
    pub unique: bool,
    /// Indexed (any index).
    #[serde(default)]
    pub indexed: bool,
    /// A virtual column of the project.
    #[serde(default, rename = "virtual")]
    pub is_virtual: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiagramNode {
    /// The table name (unique in the diagram).
    pub id: String,
    pub kind: TableKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub columns: Vec<DiagramColumn>,
    /// Columns not listed (cap or [`ColumnsMode`]).
    #[serde(default)]
    pub hidden_columns: usize,
    /// One of the tables the diagram was opened for.
    #[serde(default)]
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiagramEdge {
    /// The foreign key's name.
    pub name: String,
    /// Table with the foreign key.
    pub from: String,
    /// Referenced table.
    pub to: String,
    pub columns: Vec<String>,
    pub ref_columns: Vec<String>,
    /// Polyline from the foreign key column to the referenced column.
    pub points: Vec<Point>,
    #[serde(default, rename = "virtual")]
    pub is_virtual: bool,
    /// A nullable foreign key (zero or one), drawn dashed.
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Diagram {
    pub schema: String,
    pub nodes: Vec<DiagramNode>,
    pub edges: Vec<DiagramEdge>,
    pub width: f64,
    pub height: f64,
    /// Foreign keys to tables outside the diagram (other schemas or not shown).
    #[serde(default)]
    pub external_edges: usize,
}

fn find<'a>(schema: &'a SchemaInfo, name: &str) -> Option<&'a TableInfo> {
    schema.table(name)
}

/// The tables of the diagram, in schema order.
fn select<'a>(req: &'a DiagramRequest) -> (Vec<&'a TableInfo>, BTreeSet<String>) {
    let s = &req.schema;
    let shown =
        |t: &TableInfo| t.kind != TableKind::Collection && !(req.hide_views && t.kind.is_view());
    if req.tables.is_empty() {
        return (
            s.tables.iter().filter(|t| shown(t)).collect(),
            BTreeSet::new(),
        );
    }
    let chosen: BTreeSet<String> = req
        .tables
        .iter()
        .filter_map(|n| find(s, n).map(|t| t.name.clone()))
        .collect();
    let mut names = chosen.clone();
    if req.neighbours {
        for t in &s.tables {
            for fk in &t.foreign_keys {
                if !same_schema(s, &fk.ref_schema) {
                    continue;
                }
                let Some(target) = find(s, &fk.ref_table) else {
                    continue;
                };
                if chosen.contains(&t.name) {
                    names.insert(target.name.clone());
                }
                if chosen.contains(&target.name) {
                    names.insert(t.name.clone());
                }
            }
        }
    }
    (
        s.tables
            .iter()
            .filter(|t| names.contains(&t.name) && shown(t))
            .collect(),
        chosen,
    )
}

fn same_schema(s: &SchemaInfo, ref_schema: &str) -> bool {
    ref_schema.is_empty() || ref_schema == s.name || ref_schema.eq_ignore_ascii_case(&s.name)
}

fn node_of(t: &TableInfo, mode: ColumnsMode, selected: bool) -> DiagramNode {
    let fk_cols: BTreeSet<String> = t
        .foreign_keys
        .iter()
        .flat_map(|f| f.columns.iter().map(|c| c.to_ascii_lowercase()))
        .collect();
    let has = |list: &[String], c: &str| list.iter().any(|x| x.eq_ignore_ascii_case(c));
    let all: Vec<DiagramColumn> = t
        .columns
        .iter()
        .map(|c| DiagramColumn {
            name: c.name.clone(),
            data_type: c.data_type.clone(),
            type_label: if c.data_type.chars().count() > MAX_TYPE_CHARS {
                format!(
                    "{}…",
                    c.data_type
                        .chars()
                        .take(MAX_TYPE_CHARS - 1)
                        .collect::<String>()
                )
            } else {
                c.data_type.clone()
            },
            primary: has(&t.primary_key, &c.name),
            foreign: fk_cols.contains(&c.name.to_ascii_lowercase()),
            nullable: c.nullable,
            unique: t
                .indexes
                .iter()
                .any(|i| i.unique && !i.primary && has(&i.columns, &c.name)),
            indexed: t.indexes.iter().any(|i| has(&i.columns, &c.name)),
            is_virtual: c.is_virtual,
        })
        .collect();
    let total = all.len();
    let mut columns: Vec<DiagramColumn> = match mode {
        ColumnsMode::All => all,
        ColumnsMode::Keys => all.into_iter().filter(|c| c.primary || c.foreign).collect(),
        ColumnsMode::None => Vec::new(),
    };
    columns.truncate(MAX_COLUMNS);
    let hidden = if mode == ColumnsMode::None {
        0
    } else {
        total - columns.len()
    };
    let text = columns
        .iter()
        .map(|c| c.name.chars().count() + c.type_label.chars().count() + 6)
        .chain([t.name.chars().count() + 5])
        .max()
        .unwrap_or(0);
    let width = (text as f64 * CHAR_WIDTH + 2.0 * PADDING)
        .clamp(MIN_WIDTH, MAX_WIDTH)
        .ceil();
    let rows = columns.len() + usize::from(hidden > 0);
    let height = HEADER_HEIGHT + rows as f64 * ROW_HEIGHT + if rows > 0 { 6.0 } else { 0.0 };
    DiagramNode {
        id: t.name.clone(),
        kind: t.kind,
        comment: t.comment.clone(),
        x: 0.0,
        y: 0.0,
        width,
        height,
        columns,
        hidden_columns: hidden,
        selected,
    }
}

/// Edges between shown tables: (child index, parent index).
type Links = Vec<(usize, usize)>;

/// Layers of one connected component: referenced tables first. Cycles are broken at the
/// edge that closes them in a depth-first walk.
fn layers(members: &[usize], links: &Links) -> Vec<Vec<usize>> {
    let set: BTreeSet<usize> = members.iter().copied().collect();
    let mut parents: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &(c, p) in links {
        if c != p && set.contains(&c) && set.contains(&p) {
            parents.entry(c).or_default().push(p);
        }
    }
    // Drop back edges.
    let mut state: HashMap<usize, u8> = HashMap::new(); // 1 = on stack, 2 = done
    let mut keep: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    fn walk(
        n: usize,
        parents: &BTreeMap<usize, Vec<usize>>,
        state: &mut HashMap<usize, u8>,
        keep: &mut BTreeMap<usize, Vec<usize>>,
    ) {
        state.insert(n, 1);
        for &p in parents.get(&n).map(|v| v.as_slice()).unwrap_or(&[]) {
            match state.get(&p) {
                Some(1) => {} // back edge: ignored for layering
                Some(_) => keep.entry(n).or_default().push(p),
                None => {
                    keep.entry(n).or_default().push(p);
                    walk(p, parents, state, keep);
                }
            }
        }
        state.insert(n, 2);
    }
    for &m in members {
        if !state.contains_key(&m) {
            walk(m, &parents, &mut state, &mut keep);
        }
    }
    let mut level: HashMap<usize, usize> = HashMap::new();
    fn depth(
        n: usize,
        keep: &BTreeMap<usize, Vec<usize>>,
        level: &mut HashMap<usize, usize>,
    ) -> usize {
        if let Some(&l) = level.get(&n) {
            return l;
        }
        let l = keep
            .get(&n)
            .map(|ps| {
                ps.iter()
                    .map(|&p| depth(p, keep, level) + 1)
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        level.insert(n, l);
        l
    }
    let mut out: Vec<Vec<usize>> = Vec::new();
    for &m in members {
        let l = depth(m, &keep, &mut level);
        if out.len() <= l {
            out.resize(l + 1, Vec::new());
        }
        out[l].push(m);
    }
    out
}

/// Order each layer by the mean position of its neighbours in the layer above (down
/// sweeps) and below (up sweeps), which keeps edges short and crossings few.
fn order(layers: &mut [Vec<usize>], links: &Links) {
    let neighbours = |n: usize| -> Vec<usize> {
        links
            .iter()
            .filter_map(|&(c, p)| match (c == n, p == n) {
                (true, false) => Some(p),
                (false, true) => Some(c),
                _ => None,
            })
            .collect()
    };
    let position = |layer: &[usize]| -> HashMap<usize, f64> {
        layer
            .iter()
            .enumerate()
            .map(|(i, &n)| (n, i as f64))
            .collect()
    };
    for sweep in 0..6 {
        let down = sweep % 2 == 0;
        let range: Vec<usize> = if down {
            (1..layers.len()).collect()
        } else {
            (0..layers.len().saturating_sub(1)).rev().collect()
        };
        for i in range {
            let fixed = position(&layers[if down { i - 1 } else { i + 1 }]);
            let current = position(&layers[i]);
            let mut keyed: Vec<(f64, usize)> = layers[i]
                .iter()
                .map(|&n| {
                    let ps: Vec<f64> = neighbours(n)
                        .iter()
                        .filter_map(|m| fixed.get(m).copied())
                        .collect();
                    let key = if ps.is_empty() {
                        current[&n]
                    } else {
                        ps.iter().sum::<f64>() / ps.len() as f64
                    };
                    (key, n)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            layers[i] = keyed.into_iter().map(|(_, n)| n).collect();
        }
    }
}

/// Lay out one component: rows of layers, each node pulled under the centre of the
/// tables it references, overlaps pushed apart. Returns its width and height.
fn place(layers: &[Vec<usize>], nodes: &mut [DiagramNode], links: &Links) -> (f64, f64) {
    let mut y = 0.0;
    for (li, layer) in layers.iter().enumerate() {
        let mut x = 0.0;
        let row_height = layer.iter().map(|&n| nodes[n].height).fold(0.0, f64::max);
        for &n in layer {
            let mut want = x;
            if li > 0 {
                let centres: Vec<f64> = links
                    .iter()
                    .filter(|&&(c, p)| {
                        c == n && p != n && layers[..li].iter().any(|l| l.contains(&p))
                    })
                    .map(|&(_, p)| nodes[p].x + nodes[p].width / 2.0)
                    .collect();
                if !centres.is_empty() {
                    want =
                        centres.iter().sum::<f64>() / centres.len() as f64 - nodes[n].width / 2.0;
                }
            }
            let at = want.max(x);
            nodes[n].x = at;
            nodes[n].y = y;
            x = at + nodes[n].width + NODE_GAP;
        }
        y += row_height + LAYER_GAP;
    }
    // Normalize to start at 0.
    let all: Vec<usize> = layers.iter().flatten().copied().collect();
    let min_x = all
        .iter()
        .map(|&n| nodes[n].x)
        .fold(f64::INFINITY, f64::min);
    let min_x = if min_x.is_finite() { min_x } else { 0.0 };
    let mut w: f64 = 0.0;
    let mut h: f64 = 0.0;
    for &n in &all {
        nodes[n].x -= min_x;
        w = w.max(nodes[n].x + nodes[n].width);
        h = h.max(nodes[n].y + nodes[n].height);
    }
    (w, h)
}

/// Y of the middle of a column's row, or of the title bar.
fn row_y(node: &DiagramNode, column: Option<&String>) -> f64 {
    let idx = column.and_then(|c| {
        node.columns
            .iter()
            .position(|x| x.name.eq_ignore_ascii_case(c))
    });
    match idx {
        Some(i) => node.y + HEADER_HEIGHT + i as f64 * ROW_HEIGHT + ROW_HEIGHT / 2.0,
        None => node.y + HEADER_HEIGHT / 2.0,
    }
}

/// Route an edge between the column rows: out of the facing sides, or around the right side
/// when the tables overlap horizontally (and for self references). Vertical segments at the
/// same x get their own lane, so parallel edges stay apart.
fn route(
    from: &DiagramNode,
    to: &DiagramNode,
    fy: f64,
    ty: f64,
    lanes: &mut HashMap<i64, usize>,
) -> Vec<Point> {
    let p = |x: f64, y: f64| Point { x, y };
    let mut lane = |x: f64| {
        let l = lanes.entry(x.round() as i64).or_insert(0);
        *l += 1;
        (*l - 1) as f64 * 8.0
    };
    if from.id == to.id {
        let r = from.x + from.width;
        let x = r + LANE + lane(r);
        let ty = if (ty - fy).abs() < 1.0 {
            fy + ROW_HEIGHT / 2.0
        } else {
            ty
        };
        return vec![p(r, fy), p(x, fy), p(x, ty), p(r, ty)];
    }
    let (fl, fr) = (from.x, from.x + from.width);
    let (tl, tr) = (to.x, to.x + to.width);
    if fr + 2.0 * LANE <= tl {
        let mid = (fr + tl) / 2.0;
        let x = (mid + lane(mid)).min(tl - 6.0);
        vec![p(fr, fy), p(x, fy), p(x, ty), p(tl, ty)]
    } else if tr + 2.0 * LANE <= fl {
        let mid = (tr + fl) / 2.0;
        let x = (mid - lane(mid)).max(tr + 6.0);
        vec![p(fl, fy), p(x, fy), p(x, ty), p(tr, ty)]
    } else {
        let edge = fr.max(tr);
        let side = edge + LANE + lane(edge);
        vec![p(fr, fy), p(side, fy), p(side, ty), p(tr, ty)]
    }
}

/// Compute the diagram.
pub fn layout(req: &DiagramRequest) -> Diagram {
    let (tables, chosen) = select(req);
    let mut nodes: Vec<DiagramNode> = tables
        .iter()
        .map(|t| node_of(t, req.columns, chosen.contains(&t.name)))
        .collect();
    let index: HashMap<String, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.to_ascii_lowercase(), i))
        .collect();
    let mut links: Links = Vec::new();
    let mut fks = Vec::new();
    let mut external = 0;
    for (ci, t) in tables.iter().enumerate() {
        for fk in &t.foreign_keys {
            let target = same_schema(&req.schema, &fk.ref_schema)
                .then(|| index.get(&fk.ref_table.to_ascii_lowercase()))
                .flatten();
            match target {
                Some(&pi) => {
                    links.push((ci, pi));
                    fks.push((ci, pi, fk));
                }
                None => external += 1,
            }
        }
    }
    // Connected components (union-find), in table order.
    let mut parent: Vec<usize> = (0..nodes.len()).collect();
    fn root(parent: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while parent[r] != r {
            r = parent[r];
        }
        parent[i] = r;
        r
    }
    for &(a, b) in &links {
        let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..nodes.len() {
        let r = root(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    let linked: BTreeSet<usize> = links
        .iter()
        .filter(|(a, b)| a != b)
        .flat_map(|&(a, b)| [a, b])
        .collect();
    let (mut connected, isolated): (Vec<Vec<usize>>, Vec<Vec<usize>>) = groups
        .into_values()
        .partition(|g| g.iter().any(|n| linked.contains(n)));
    // Biggest components first.
    connected.sort_by_key(|g| std::cmp::Reverse(g.len()));
    let mut x = MARGIN;
    let mut height: f64 = 0.0;
    for g in &connected {
        let mut ls = layers(g, &links);
        order(&mut ls, &links);
        let (w, h) = place(&ls, &mut nodes, &links);
        for &n in g {
            nodes[n].x += x;
            nodes[n].y += MARGIN;
        }
        x += w + COMPONENT_GAP;
        height = height.max(h);
    }
    // Tables without relations: a grid below (or alone).
    let loose: Vec<usize> = isolated.into_iter().flatten().collect();
    if !loose.is_empty() {
        let per_row = ((loose.len() as f64).sqrt().ceil() as usize).max(1);
        let top = if connected.is_empty() {
            MARGIN
        } else {
            MARGIN + height + LAYER_GAP
        };
        let mut y = top;
        for row in loose.chunks(per_row) {
            let mut cx = MARGIN;
            let mut row_h: f64 = 0.0;
            for &n in row {
                nodes[n].x = cx;
                nodes[n].y = y;
                cx += nodes[n].width + NODE_GAP;
                row_h = row_h.max(nodes[n].height);
            }
            y += row_h + NODE_GAP;
        }
    }
    let mut lanes: HashMap<i64, usize> = HashMap::new();
    let edges = fks
        .into_iter()
        .map(|(ci, pi, fk)| {
            let from = &nodes[ci];
            let to = &nodes[pi];
            let fy = row_y(from, fk.columns.first());
            let ty = row_y(to, fk.ref_columns.first());
            let optional = fk.columns.iter().any(|c| {
                tables[ci]
                    .column(c)
                    .map(|col| col.nullable)
                    .unwrap_or(false)
            });
            DiagramEdge {
                name: fk.name.clone(),
                from: from.id.clone(),
                to: to.id.clone(),
                columns: fk.columns.clone(),
                ref_columns: fk.ref_columns.clone(),
                points: route(from, to, fy, ty, &mut lanes),
                is_virtual: fk.is_virtual,
                optional,
            }
        })
        .collect::<Vec<_>>();
    let mut width: f64 = 0.0;
    let mut bottom: f64 = 0.0;
    for n in &nodes {
        width = width.max(n.x + n.width);
        bottom = bottom.max(n.y + n.height);
    }
    for e in &edges {
        for p in &e.points {
            width = width.max(p.x);
        }
    }
    Diagram {
        schema: req.schema.name.clone(),
        nodes,
        edges,
        width: (width + MARGIN).ceil(),
        height: (bottom + MARGIN).ceil(),
        external_edges: external,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_dbmodel::{ColumnInfo, ForeignKey, IndexInfo};

    fn col(name: &str, ty: &str, nullable: bool) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: ty.into(),
            nullable,
            ..Default::default()
        }
    }

    fn fk(cols: &[&str], table: &str) -> ForeignKey {
        ForeignKey {
            name: format!("fk_{table}_{}", cols.join("_")),
            columns: cols.iter().map(|c| c.to_string()).collect(),
            ref_schema: "shop".into(),
            ref_table: table.into(),
            ref_columns: vec!["id".into()],
            ..Default::default()
        }
    }

    fn table(name: &str, columns: Vec<ColumnInfo>, fks: Vec<ForeignKey>) -> TableInfo {
        TableInfo {
            name: name.into(),
            kind: TableKind::Table,
            columns,
            primary_key: vec!["id".into()],
            foreign_keys: fks,
            indexes: vec![IndexInfo {
                name: format!("{name}_pkey"),
                columns: vec!["id".into()],
                unique: true,
                primary: true,
            }],
            ..Default::default()
        }
    }

    fn shop() -> SchemaInfo {
        SchemaInfo {
            name: "shop".into(),
            tables: vec![
                table(
                    "order_items",
                    vec![
                        col("id", "bigint", false),
                        col("order_id", "bigint", false),
                        col("product_id", "bigint", true),
                    ],
                    vec![fk(&["order_id"], "orders"), fk(&["product_id"], "products")],
                ),
                table(
                    "orders",
                    vec![
                        col("id", "bigint", false),
                        col("user_id", "bigint", false),
                        col("total", "numeric(10,2)", false),
                    ],
                    vec![fk(&["user_id"], "users")],
                ),
                table(
                    "products",
                    vec![
                        col("id", "bigint", false),
                        col("name", "varchar(255)", false),
                    ],
                    vec![],
                ),
                table(
                    "settings",
                    vec![col("id", "bigint", false), col("value", "text", true)],
                    vec![],
                ),
                table(
                    "users",
                    vec![
                        col("id", "bigint", false),
                        col("email", "varchar(255)", false),
                        col("manager_id", "bigint", true),
                    ],
                    vec![fk(&["manager_id"], "users")],
                ),
            ],
            ..Default::default()
        }
    }

    fn node<'a>(d: &'a Diagram, id: &str) -> &'a DiagramNode {
        d.nodes.iter().find(|n| n.id == id).unwrap()
    }

    fn overlap(a: &DiagramNode, b: &DiagramNode) -> bool {
        a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
    }

    #[test]
    fn lays_out_referenced_tables_above() {
        let d = layout(&DiagramRequest {
            schema: shop(),
            ..Default::default()
        });
        assert_eq!(d.nodes.len(), 5);
        assert_eq!(d.edges.len(), 4);
        let (users, orders, items) = (
            node(&d, "users"),
            node(&d, "orders"),
            node(&d, "order_items"),
        );
        assert!(users.y < orders.y && orders.y < items.y, "{d:#?}");
        assert!(node(&d, "products").y < items.y);
        // The table without relations goes below the others.
        assert!(node(&d, "settings").y > items.y);
        for (i, a) in d.nodes.iter().enumerate() {
            for b in &d.nodes[i + 1..] {
                assert!(!overlap(a, b), "{} overlaps {}", a.id, b.id);
            }
            assert!(a.x + a.width <= d.width && a.y + a.height <= d.height);
        }
        // Keys and markers.
        let oc = &orders.columns;
        assert!(oc[0].primary && !oc[0].foreign);
        assert!(oc[1].foreign && !oc[1].nullable);
        assert_eq!(orders.height, HEADER_HEIGHT + 3.0 * ROW_HEIGHT + 6.0);
    }

    #[test]
    fn routes_edges_between_column_rows() {
        let d = layout(&DiagramRequest {
            schema: shop(),
            ..Default::default()
        });
        let e = d
            .edges
            .iter()
            .find(|e| e.from == "orders" && e.to == "users")
            .unwrap();
        let (orders, users) = (node(&d, "orders"), node(&d, "users"));
        let first = e.points.first().unwrap();
        let last = e.points.last().unwrap();
        assert_eq!(first.y, orders.y + HEADER_HEIGHT + ROW_HEIGHT * 1.5);
        assert_eq!(last.y, users.y + HEADER_HEIGHT + ROW_HEIGHT * 0.5);
        assert!(first.x == orders.x || first.x == orders.x + orders.width);
        assert!(last.x == users.x || last.x == users.x + users.width);
        assert!(!e.optional);
        // Self reference loops on the right side.
        let s = d
            .edges
            .iter()
            .find(|e| e.from == "users" && e.to == "users")
            .unwrap();
        assert_eq!(s.points.len(), 4);
        assert!(s.points[1].x > users.x + users.width);
        assert!(s.optional);
        let items = d.edges.iter().find(|e| e.to == "products").unwrap();
        assert!(items.optional);
    }

    #[test]
    fn selected_tables_with_neighbours_and_column_modes() {
        let d = layout(&DiagramRequest {
            schema: shop(),
            tables: vec!["ORDERS".into()],
            neighbours: true,
            columns: ColumnsMode::Keys,
            ..Default::default()
        });
        let ids: BTreeSet<&str> = d.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, BTreeSet::from(["order_items", "orders", "users"]));
        assert!(node(&d, "orders").selected && !node(&d, "users").selected);
        assert_eq!(node(&d, "users").columns.len(), 2); // id, manager_id
        assert_eq!(node(&d, "users").hidden_columns, 1);
        // order_items → products leaves the diagram.
        assert_eq!(d.external_edges, 1);
        let alone = layout(&DiagramRequest {
            schema: shop(),
            tables: vec!["settings".into()],
            columns: ColumnsMode::None,
            ..Default::default()
        });
        assert_eq!(alone.nodes.len(), 1);
        assert_eq!(alone.nodes[0].height, HEADER_HEIGHT);
        assert_eq!(alone.nodes[0].x, MARGIN);
    }

    #[test]
    fn survives_cycles_and_caps_columns() {
        let mut s = shop();
        // users → orders closes a cycle users → orders → users.
        s.tables[4]
            .foreign_keys
            .push(fk(&["last_order_id"], "orders"));
        s.tables[3].columns = (0..40)
            .map(|i| col(&format!("c{i}"), "int", true))
            .collect();
        let d = layout(&DiagramRequest {
            schema: s,
            ..Default::default()
        });
        assert_eq!(d.edges.len(), 5);
        let settings = node(&d, "settings");
        assert_eq!(settings.columns.len(), MAX_COLUMNS);
        assert_eq!(settings.hidden_columns, 10);
        let mut s = shop();
        s.tables[1].columns[2].data_type = "enum('pending','paid','shipped','delivered')".into();
        let d = layout(&DiagramRequest {
            schema: s,
            ..Default::default()
        });
        let c = &node(&d, "orders").columns[2];
        assert_eq!(c.type_label, "enum('pending','paid','…");
        assert_eq!(c.type_label.chars().count(), MAX_TYPE_CHARS);
        for (i, a) in d.nodes.iter().enumerate() {
            for b in &d.nodes[i + 1..] {
                assert!(!overlap(a, b), "{} overlaps {}", a.id, b.id);
            }
        }
    }
}
