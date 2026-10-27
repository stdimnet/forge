//! Explain Plan: the `EXPLAIN` statement of each dialect for the statement at the caret, and
//! its output parsed into a tree of operations with estimated cost and rows, plus the bars
//! of the flame-style view (DataGrip's "Show Visualization").
//!
//! | Dialect | Statement | Parsed from |
//! | --- | --- | --- |
//! | PostgreSQL | `EXPLAIN (FORMAT JSON)` | `Plan` / `Plans` objects |
//! | MySQL / MariaDB | `EXPLAIN FORMAT=TREE` (MariaDB: `EXPLAIN`) | the indented tree, or the classic table |
//! | SQLite | `EXPLAIN QUERY PLAN` | `id` / `parent` / `detail` rows |
//! | SQL Server | `SET SHOWPLAN_ALL ON` | `NodeId` / `Parent` rows with `TotalSubtreeCost` |
//! | ClickHouse | `EXPLAIN json = 1, description = 1` | `Plan` / `Plans` objects |
//! | Oracle | `EXPLAIN PLAN FOR` | `PLAN_TABLE` rows (`id`, `parent_id`) |
//!
//! "Explain Plan (Raw)" runs the plain `EXPLAIN` and shows its rows as they are. Nothing is
//! ever run with `ANALYZE`: explaining a statement does not execute it.

use std::collections::BTreeMap;

use forge_sql::{leading_keyword, Dialect};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The statements to run for a plan; the rows of `statements[result]` are the plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainScript {
    pub statements: Vec<String>,
    pub result: usize,
    /// Statements that must run afterwards even when the plan failed (`SET SHOWPLAN_ALL OFF`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cleanup: Vec<String>,
    /// Tried when `statements[result]` fails (MariaDB has no `FORMAT=TREE`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanFormat {
    /// PostgreSQL and ClickHouse JSON plans.
    Json,
    /// MySQL `EXPLAIN FORMAT=TREE`.
    MySqlTree,
    /// The classic MySQL / MariaDB `EXPLAIN` table.
    MySqlTable,
    SqliteQueryPlan,
    SqlServerShowplan,
    OraclePlanTable,
    /// Plain text lines (PostgreSQL's text `EXPLAIN`): one node per line, nested by indent.
    Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanProperty {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlanNode {
    /// Pre-order number, also the id of its flame bar.
    pub id: usize,
    /// `Seq Scan`, `Nested loop inner join`, `SCAN`, `Clustered Index Seek`, …
    pub operation: String,
    /// The table or index it reads.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object: String,
    /// Conditions and other details.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    /// Estimated total cost (including the children).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup_cost: Option<f64>,
    /// Cost of this operation alone: its cost minus its children's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_cost: Option<f64>,
    /// Estimated rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<PlanProperty>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<PlanNode>,
}

/// One bar of the flame-style view: `start` and `width` are fractions of the full width.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlameBar {
    pub id: usize,
    pub depth: usize,
    pub start: f64,
    pub width: f64,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub format: PlanFormat,
    pub root: PlanNode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_cost: Option<f64>,
    /// What the bar widths stand for: `cost`, `rows` or `count` (equal shares).
    pub weight: String,
    pub flame: Vec<FlameBar>,
}

fn strip(stmt: &str) -> String {
    stmt.trim().trim_end_matches(';').trim_end().to_string()
}

/// The statements that explain `stmt`; `raw` for "Explain Plan (Raw)".
pub fn explain_script(d: Dialect, stmt: &str, raw: bool) -> Result<ExplainScript, String> {
    let s = strip(stmt);
    if s.is_empty() {
        return Err("There is no statement to explain".into());
    }
    let single = |sql: String| ExplainScript {
        statements: vec![sql],
        result: 0,
        cleanup: Vec::new(),
        fallback: None,
    };
    // A statement that already is an EXPLAIN runs as it is.
    if leading_keyword(&s) == "EXPLAIN" {
        return Ok(single(s));
    }
    Ok(match d {
        Dialect::Postgres if raw => single(format!("EXPLAIN {s}")),
        Dialect::Postgres => single(format!("EXPLAIN (FORMAT JSON) {s}")),
        Dialect::MySql if raw => single(format!("EXPLAIN {s}")),
        Dialect::MySql => ExplainScript {
            fallback: Some(format!("EXPLAIN {s}")),
            ..single(format!("EXPLAIN FORMAT=TREE {s}"))
        },
        Dialect::Sqlite => single(format!("EXPLAIN QUERY PLAN {s}")),
        Dialect::SqlServer => ExplainScript {
            statements: vec!["SET SHOWPLAN_ALL ON".into(), s],
            result: 1,
            cleanup: vec!["SET SHOWPLAN_ALL OFF".into()],
            fallback: None,
        },
        Dialect::ClickHouse if raw => single(format!("EXPLAIN {s}")),
        Dialect::ClickHouse => single(format!("EXPLAIN json = 1, description = 1 {s}")),
        Dialect::Oracle => {
            return Err(
                "Explain Plan is not available for Oracle: Forge runs every Oracle statement in its own SQL*Plus session, so the plan table is empty afterwards".into(),
            )
        }
        Dialect::Redis | Dialect::Mongo => return Err("Explain Plan is only available for SQL data sources".into()),
    })
}

fn text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn column(columns: &[String], name: &str) -> Option<usize> {
    columns.iter().position(|c| c.eq_ignore_ascii_case(name))
}

// ---- JSON (PostgreSQL, ClickHouse) ------------------------------------------------------

fn json_node(v: &Value) -> PlanNode {
    let o = v.as_object().cloned().unwrap_or_default();
    let get = |k: &str| o.get(k).map(text).unwrap_or_default();
    let mut object = get("Relation Name");
    if object.is_empty() {
        object = get("Index Name");
    } else if !get("Alias").is_empty() && get("Alias") != object {
        object = format!("{object} {}", get("Alias"));
    }
    let detail = [
        "Index Cond",
        "Hash Cond",
        "Merge Cond",
        "Join Filter",
        "Filter",
        "Recheck Cond",
        "Sort Key",
        "Group Key",
        "Description",
    ]
    .iter()
    .filter_map(|k| o.get(*k).map(|v| (k, v)))
    .map(|(k, v)| {
        let t = match v {
            Value::Array(items) => items.iter().map(text).collect::<Vec<_>>().join(", "),
            other => text(other),
        };
        if *k == "Description" {
            t
        } else {
            format!("{k}: {t}")
        }
    })
    .collect::<Vec<_>>()
    .join("; ");
    let mut operation = get("Node Type");
    if let Some(j) = o
        .get("Join Type")
        .map(text)
        .filter(|j| !j.is_empty() && !operation.contains("Join") && !operation.contains("Loop"))
    {
        operation = format!("{operation} ({j})");
    }
    if operation.is_empty() {
        operation = "Plan".into();
    }
    let properties = o
        .iter()
        .filter(|(k, v)| {
            !matches!(k.as_str(), "Plans" | "Node Type") && !v.is_object() && !v.is_array()
        })
        .map(|(k, v)| PlanProperty {
            name: k.clone(),
            value: text(v),
        })
        .collect();
    PlanNode {
        operation,
        object,
        detail,
        cost: o.get("Total Cost").and_then(number),
        startup_cost: o.get("Startup Cost").and_then(number),
        rows: o.get("Plan Rows").and_then(number),
        properties,
        children: o
            .get("Plans")
            .and_then(Value::as_array)
            .map(|ps| ps.iter().map(json_node).collect())
            .unwrap_or_default(),
        ..Default::default()
    }
}

fn parse_json(textual: &str) -> Result<PlanNode, String> {
    let v: Value =
        serde_json::from_str(textual.trim()).map_err(|e| format!("The plan is not JSON: {e}"))?;
    let first = match &v {
        Value::Array(a) => a.first().cloned().unwrap_or(Value::Null),
        other => other.clone(),
    };
    let plan = first.get("Plan").cloned().unwrap_or(first);
    if !plan.is_object() {
        return Err("The JSON plan has no Plan object".into());
    }
    Ok(json_node(&plan))
}

// ---- MySQL tree and indented text -------------------------------------------------------

/// `Nested loop inner join  (cost=1.2 rows=3)` → operation and the numbers.
fn split_costs(line: &str) -> (String, Option<f64>, Option<f64>, Option<f64>) {
    let mut op = line.trim().to_string();
    let (mut cost, mut startup, mut rows) = (None, None, None);
    // PostgreSQL text: `(cost=0.00..35.50 rows=2550 width=4)`; MySQL: `(cost=1.2 rows=3)`.
    while let Some(open) = op.rfind("  (").or_else(|| op.rfind(" (cost=")) {
        let inner = op[open..]
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .to_string();
        if !inner.contains('=') {
            break;
        }
        for part in inner.split_whitespace() {
            if let Some((k, v)) = part.split_once('=') {
                match k {
                    "cost" => match v.split_once("..") {
                        Some((a, b)) => {
                            startup = a.parse().ok();
                            cost = b.parse().ok();
                        }
                        None => cost = v.parse().ok(),
                    },
                    "rows" => rows = rows.or(v.parse().ok()),
                    _ => {}
                }
            }
        }
        op.truncate(open);
        op = op.trim_end().to_string();
    }
    (op, cost, startup, rows)
}

/// `Table scan on u`, `Index lookup on o using user_id (user_id=u.id)` → (`Table scan`, `u`,
/// `using user_id (user_id=u.id)`).
fn split_object(op: &str) -> (String, String, String) {
    if let Some(i) = op.find(" on ") {
        let rest = &op[i + 4..];
        let (obj, detail) = match rest.find(' ') {
            Some(j) => (&rest[..j], rest[j + 1..].trim()),
            None => (rest, ""),
        };
        return (op[..i].to_string(), obj.to_string(), detail.to_string());
    }
    if let Some(i) = op.find(": ") {
        return (op[..i].to_string(), String::new(), op[i + 2..].to_string());
    }
    (op.to_string(), String::new(), String::new())
}

fn parse_indented(lines: &[String], mysql: bool) -> Result<PlanNode, String> {
    // (indent, node); details of PostgreSQL text plans (`Filter: …`) attach to the last node.
    let mut stack: Vec<(usize, PlanNode)> = Vec::new();
    let mut roots: Vec<PlanNode> = Vec::new();
    fn close_to(stack: &mut Vec<(usize, PlanNode)>, roots: &mut Vec<PlanNode>, indent: usize) {
        while stack.last().is_some_and(|(i, _)| *i >= indent) {
            let (_, n) = stack.pop().expect("non-empty");
            match stack.last_mut() {
                Some((_, p)) => p.children.push(n),
                None => roots.push(n),
            }
        }
    }
    for raw in lines.iter().flat_map(|l| l.lines()) {
        if raw.trim().is_empty() {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        let t = raw.trim();
        let is_node = t.starts_with("->")
            || stack.is_empty()
            || (!mysql && !t.contains(": ") && t.contains("(cost="));
        if !is_node {
            if let Some((_, n)) = stack.last_mut() {
                if !n.detail.is_empty() {
                    n.detail.push_str("; ");
                }
                n.detail.push_str(t);
            }
            continue;
        }
        let body = t.trim_start_matches("->").trim();
        let (op, cost, startup, rows) = split_costs(body);
        let (operation, object, detail) = if mysql {
            split_object(&op)
        } else {
            (op.clone(), String::new(), String::new())
        };
        let (operation, object) = if !mysql {
            // PostgreSQL text: `Seq Scan on users u`.
            match operation.find(" on ") {
                Some(i) => (operation[..i].to_string(), operation[i + 4..].to_string()),
                None => (operation, object),
            }
        } else {
            (operation, object)
        };
        close_to(&mut stack, &mut roots, indent);
        stack.push((
            indent,
            PlanNode {
                operation,
                object,
                detail,
                cost,
                startup_cost: startup,
                rows,
                ..Default::default()
            },
        ));
    }
    close_to(&mut stack, &mut roots, 0);
    match roots.len() {
        0 => Err("The plan is empty".into()),
        1 => Ok(roots.pop().expect("one root")),
        _ => Ok(PlanNode {
            operation: "Query".into(),
            children: roots,
            ..Default::default()
        }),
    }
}

// ---- row-based plans --------------------------------------------------------------------

/// Build a tree from rows with an id and a parent id.
fn tree_from_rows(mut items: Vec<(i64, i64, PlanNode)>, root: PlanNode) -> PlanNode {
    let ids: Vec<i64> = items.iter().map(|(id, _, _)| *id).collect();
    let mut children: BTreeMap<i64, Vec<PlanNode>> = BTreeMap::new();
    // Children first: process in reverse so each node gets its children before its parent.
    items.sort_by_key(|(id, _, _)| std::cmp::Reverse(*id));
    let mut top = Vec::new();
    for (id, parent, mut node) in items {
        let mut kids = children.remove(&id).unwrap_or_default();
        kids.reverse();
        node.children = kids;
        if parent != id && ids.contains(&parent) {
            children.entry(parent).or_default().push(node);
        } else {
            top.push(node);
        }
    }
    top.reverse();
    if top.len() == 1 && root.operation.is_empty() {
        return top.pop().expect("one");
    }
    PlanNode {
        children: top,
        ..root
    }
}

fn parse_sqlite(columns: &[String], rows: &[Vec<Value>]) -> Result<PlanNode, String> {
    let (Some(ci), Some(cp), Some(cd)) = (
        column(columns, "id"),
        column(columns, "parent"),
        column(columns, "detail"),
    ) else {
        return Err("Not an EXPLAIN QUERY PLAN result".into());
    };
    let items = rows
        .iter()
        .map(|r| {
            let detail = text(&r[cd]);
            let mut words = detail.splitn(3, ' ');
            let first = words.next().unwrap_or_default().to_string();
            let (operation, object, rest) = if matches!(first.as_str(), "SCAN" | "SEARCH") {
                let obj = words.next().unwrap_or_default().to_string();
                (first, obj, words.next().unwrap_or_default().to_string())
            } else {
                (detail.clone(), String::new(), String::new())
            };
            let node = PlanNode {
                operation,
                object,
                detail: rest,
                ..Default::default()
            };
            (
                number(&r[ci]).unwrap_or(0.0) as i64,
                number(&r[cp]).unwrap_or(0.0) as i64,
                node,
            )
        })
        .collect();
    Ok(tree_from_rows(
        items,
        PlanNode {
            operation: "QUERY PLAN".into(),
            ..Default::default()
        },
    ))
}

fn parse_showplan(columns: &[String], rows: &[Vec<Value>]) -> Result<PlanNode, String> {
    let (Some(ci), Some(cp)) = (column(columns, "NodeId"), column(columns, "Parent")) else {
        return Err("Not a SHOWPLAN_ALL result".into());
    };
    let get = |r: &Vec<Value>, n: &str| column(columns, n).map(|i| text(&r[i])).unwrap_or_default();
    let num = |r: &Vec<Value>, n: &str| column(columns, n).and_then(|i| number(&r[i]));
    let items = rows
        .iter()
        .map(|r| {
            let physical = get(r, "PhysicalOp");
            let argument = get(r, "Argument");
            let operation = if physical.is_empty() {
                get(r, "StmtText").trim().to_string()
            } else {
                physical
            };
            // OBJECT:([shop].[dbo].[users].[PK_users] AS [u])
            let object = argument
                .split("OBJECT:(")
                .nth(1)
                .map(|s| {
                    let parts: Vec<&str> = s
                        .split(['[', ']', '.'])
                        .filter(|p| !p.trim().is_empty() && !p.contains(')'))
                        .collect();
                    parts
                        .get(2)
                        .or(parts.last())
                        .map(|p| p.to_string())
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            let mut properties = Vec::new();
            for name in [
                "LogicalOp",
                "EstimateIO",
                "EstimateCPU",
                "AvgRowSize",
                "EstimateExecutions",
            ] {
                let v = get(r, name);
                if !v.is_empty() {
                    properties.push(PlanProperty {
                        name: name.into(),
                        value: v,
                    });
                }
            }
            let node = PlanNode {
                operation,
                object,
                detail: argument,
                cost: num(r, "TotalSubtreeCost"),
                rows: num(r, "EstimateRows"),
                properties,
                ..Default::default()
            };
            (
                number(&r[ci]).unwrap_or(0.0) as i64,
                number(&r[cp]).unwrap_or(-1.0) as i64,
                node,
            )
        })
        .collect();
    Ok(tree_from_rows(items, PlanNode::default()))
}

fn parse_oracle(columns: &[String], rows: &[Vec<Value>]) -> Result<PlanNode, String> {
    let (Some(ci), Some(cp)) = (column(columns, "ID"), column(columns, "PARENT_ID")) else {
        return Err("Not a PLAN_TABLE result".into());
    };
    let get = |r: &Vec<Value>, n: &str| column(columns, n).map(|i| text(&r[i])).unwrap_or_default();
    let num = |r: &Vec<Value>, n: &str| column(columns, n).and_then(|i| number(&r[i]));
    let items = rows
        .iter()
        .map(|r| {
            let options = get(r, "OPTIONS");
            let node = PlanNode {
                operation: format!("{} {options}", get(r, "OPERATION"))
                    .trim()
                    .to_string(),
                object: get(r, "OBJECT_NAME"),
                cost: num(r, "COST"),
                rows: num(r, "CARDINALITY"),
                ..Default::default()
            };
            (
                number(&r[ci]).unwrap_or(0.0) as i64,
                number(&r[cp]).unwrap_or(-1.0) as i64,
                node,
            )
        })
        .collect();
    Ok(tree_from_rows(items, PlanNode::default()))
}

fn mysql_access(t: &str) -> &str {
    match t {
        "ALL" => "Full table scan",
        "index" => "Full index scan",
        "range" => "Index range scan",
        "ref" | "ref_or_null" => "Index lookup",
        "eq_ref" => "Unique index lookup",
        "const" | "system" => "Constant row",
        "fulltext" => "Full-text index",
        "index_merge" => "Index merge",
        "" => "Operation",
        other => other,
    }
}

fn parse_mysql_table(columns: &[String], rows: &[Vec<Value>]) -> Result<PlanNode, String> {
    if column(columns, "select_type").is_none() {
        return Err("Not an EXPLAIN result".into());
    }
    let get = |r: &Vec<Value>, n: &str| column(columns, n).map(|i| text(&r[i])).unwrap_or_default();
    let children = rows
        .iter()
        .map(|r| {
            let key = get(r, "key");
            let extra = get(r, "Extra");
            let mut detail = Vec::new();
            if !key.is_empty() {
                detail.push(format!("key: {key}"));
            }
            if !extra.is_empty() {
                detail.push(extra);
            }
            let properties = columns
                .iter()
                .enumerate()
                .filter(|(i, _)| !r[*i].is_null())
                .map(|(i, c)| PlanProperty {
                    name: c.clone(),
                    value: text(&r[i]),
                })
                .collect();
            PlanNode {
                operation: mysql_access(&get(r, "type")).to_string(),
                object: get(r, "table"),
                detail: detail.join("; "),
                rows: column(columns, "rows").and_then(|i| number(&r[i])),
                properties,
                ..Default::default()
            }
        })
        .collect();
    Ok(PlanNode {
        operation: "Query".into(),
        children,
        ..Default::default()
    })
}

// ---- tree post-processing ---------------------------------------------------------------

fn number_nodes(n: &mut PlanNode, next: &mut usize) {
    n.id = *next;
    *next += 1;
    let child_cost: Option<f64> = n.children.iter().map(|c| c.cost).sum();
    n.self_cost = match (n.cost, child_cost) {
        (Some(c), Some(k)) => Some((c - k).max(0.0)),
        (Some(c), None) if n.children.is_empty() => Some(c),
        _ => None,
    };
    for c in &mut n.children {
        number_nodes(c, next);
    }
}

fn all_have(n: &PlanNode, f: &dyn Fn(&PlanNode) -> bool) -> bool {
    f(n) && n.children.iter().all(|c| all_have(c, f))
}

fn bars(
    n: &PlanNode,
    depth: usize,
    start: f64,
    width: f64,
    weight: &dyn Fn(&PlanNode) -> f64,
    out: &mut Vec<FlameBar>,
) {
    let label = if n.object.is_empty() {
        n.operation.clone()
    } else {
        format!("{} {}", n.operation, n.object)
    };
    out.push(FlameBar {
        id: n.id,
        depth,
        start,
        width,
        label,
        cost: n.cost,
        rows: n.rows,
    });
    let total: f64 = n.children.iter().map(weight).sum();
    let own = weight(n).max(total);
    let mut x = start;
    for c in &n.children {
        let w = if own > 0.0 {
            width * weight(c) / own
        } else {
            width / n.children.len() as f64
        };
        bars(c, depth + 1, x, w, weight, out);
        x += w;
    }
}

/// What a flame bar's width is proportional to.
type Weight = Box<dyn Fn(&PlanNode) -> f64>;

/// Parse the rows of an explain statement into a plan.
pub fn parse_plan(columns: &[String], rows: &[Vec<Value>]) -> Result<Plan, String> {
    if rows.is_empty() {
        return Err("The plan is empty".into());
    }
    let single_text = || -> Vec<String> {
        rows.iter()
            .map(|r| r.iter().map(text).collect::<Vec<_>>().join(" "))
            .collect()
    };
    let (format, mut root) =
        if column(columns, "detail").is_some() && column(columns, "parent").is_some() {
            (PlanFormat::SqliteQueryPlan, parse_sqlite(columns, rows)?)
        } else if column(columns, "NodeId").is_some() {
            (
                PlanFormat::SqlServerShowplan,
                parse_showplan(columns, rows)?,
            )
        } else if column(columns, "PARENT_ID").is_some() && column(columns, "OPERATION").is_some() {
            (PlanFormat::OraclePlanTable, parse_oracle(columns, rows)?)
        } else if column(columns, "select_type").is_some() {
            (PlanFormat::MySqlTable, parse_mysql_table(columns, rows)?)
        } else {
            let lines = single_text();
            let joined = lines.join("\n");
            let t = joined.trim_start();
            if t.starts_with('[') || t.starts_with('{') {
                (PlanFormat::Json, parse_json(&joined)?)
            } else if t.starts_with("->") {
                (PlanFormat::MySqlTree, parse_indented(&lines, true)?)
            } else {
                (PlanFormat::Text, parse_indented(&lines, false)?)
            }
        };
    let mut next = 0;
    number_nodes(&mut root, &mut next);
    let (name, weight): (&str, Weight) = if all_have(&root, &|n| n.cost.is_some()) {
        ("cost", Box::new(|n: &PlanNode| n.cost.unwrap_or(0.0)))
    } else if all_have(&root, &|n| n.rows.is_some() || !n.children.is_empty())
        && root.children.iter().any(|c| c.rows.is_some())
    {
        ("rows", Box::new(|n: &PlanNode| subtree_rows(n)))
    } else {
        ("count", Box::new(|n: &PlanNode| leaves(n) as f64))
    };
    let mut flame = Vec::new();
    bars(&root, 0, 0.0, 1.0, &*weight, &mut flame);
    Ok(Plan {
        format,
        total_cost: root.cost,
        root,
        weight: name.into(),
        flame,
    })
}

fn subtree_rows(n: &PlanNode) -> f64 {
    n.rows
        .unwrap_or(0.0)
        .max(n.children.iter().map(subtree_rows).sum())
}

fn leaves(n: &PlanNode) -> usize {
    if n.children.is_empty() {
        1
    } else {
        n.children.iter().map(leaves).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cols(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn explain_statements_per_dialect() {
        let s = "SELECT * FROM users WHERE id = 1;";
        assert_eq!(
            explain_script(Dialect::Postgres, s, false)
                .unwrap()
                .statements,
            ["EXPLAIN (FORMAT JSON) SELECT * FROM users WHERE id = 1"]
        );
        assert_eq!(
            explain_script(Dialect::Postgres, s, true)
                .unwrap()
                .statements,
            ["EXPLAIN SELECT * FROM users WHERE id = 1"]
        );
        let my = explain_script(Dialect::MySql, s, false).unwrap();
        assert_eq!(
            my.statements[0],
            "EXPLAIN FORMAT=TREE SELECT * FROM users WHERE id = 1"
        );
        assert_eq!(
            my.fallback.as_deref(),
            Some("EXPLAIN SELECT * FROM users WHERE id = 1")
        );
        assert_eq!(
            explain_script(Dialect::Sqlite, s, true).unwrap().statements,
            ["EXPLAIN QUERY PLAN SELECT * FROM users WHERE id = 1"]
        );
        let ss = explain_script(Dialect::SqlServer, s, false).unwrap();
        assert_eq!(
            (ss.statements.len(), ss.result, ss.cleanup.as_slice()),
            (2, 1, ["SET SHOWPLAN_ALL OFF".to_string()].as_slice())
        );
        assert!(explain_script(Dialect::ClickHouse, s, false)
            .unwrap()
            .statements[0]
            .starts_with("EXPLAIN json = 1"));
        assert!(explain_script(Dialect::Oracle, s, false).is_err());
        assert!(explain_script(Dialect::Redis, "GET a", false).is_err());
        assert!(explain_script(Dialect::Postgres, "  ; ", false).is_err());
        // Already an EXPLAIN: as it is.
        assert_eq!(
            explain_script(Dialect::Postgres, "explain select 1", false)
                .unwrap()
                .statements,
            ["explain select 1"]
        );
    }

    #[test]
    fn postgres_json_plan() {
        let plan = json!([{"Plan": {
        "Node Type": "Hash Join", "Join Type": "Inner", "Startup Cost": 1.5, "Total Cost": 40.0, "Plan Rows": 120, "Hash Cond": "(o.user_id = u.id)",
        "Plans": [
            {"Node Type": "Seq Scan", "Relation Name": "orders", "Alias": "o", "Total Cost": 25.0, "Plan Rows": 1200, "Filter": "(total > 10)"},
            {"Node Type": "Hash", "Total Cost": 12.0, "Plan Rows": 100, "Plans": [
                {"Node Type": "Seq Scan", "Relation Name": "users", "Alias": "users", "Total Cost": 10.0, "Plan Rows": 100}
            ]}
        ]}}]);
        let p = parse_plan(
            &cols(&["QUERY PLAN"]),
            &[vec![Value::String(plan.to_string())]],
        )
        .unwrap();
        assert_eq!(p.format, PlanFormat::Json);
        assert_eq!(p.total_cost, Some(40.0));
        assert_eq!(p.root.operation, "Hash Join");
        assert_eq!(p.root.detail, "Hash Cond: (o.user_id = u.id)");
        assert_eq!(p.root.self_cost, Some(3.0));
        let scan = &p.root.children[0];
        assert_eq!(
            (scan.operation.as_str(), scan.object.as_str(), scan.rows),
            ("Seq Scan", "orders o", Some(1200.0))
        );
        assert_eq!(scan.detail, "Filter: (total > 10)");
        assert_eq!(p.root.children[1].children[0].object, "users");
        assert_eq!(p.weight, "cost");
        assert_eq!(p.flame.len(), 4);
        assert_eq!(p.flame[0].width, 1.0);
        let orders = p
            .flame
            .iter()
            .find(|b| b.label == "Seq Scan orders o")
            .unwrap();
        assert_eq!(
            (orders.depth, orders.start, orders.width),
            (1, 0.0, 25.0 / 40.0)
        );
        let ids: Vec<usize> = p.flame.iter().map(|b| b.id).collect();
        assert_eq!(ids, [0, 1, 2, 3]);
    }

    #[test]
    fn mysql_tree_and_table() {
        let tree = "-> Nested loop inner join  (cost=1.15 rows=3)\n    -> Table scan on u  (cost=0.55 rows=3)\n    -> Index lookup on o using user_id (user_id=u.id)  (cost=0.2 rows=1)\n";
        let p = parse_plan(&cols(&["EXPLAIN"]), &[vec![json!(tree)]]).unwrap();
        assert_eq!(p.format, PlanFormat::MySqlTree);
        assert_eq!(p.root.operation, "Nested loop inner join");
        assert_eq!((p.root.cost, p.root.rows), (Some(1.15), Some(3.0)));
        let kids: Vec<(&str, &str, &str)> = p
            .root
            .children
            .iter()
            .map(|c| (c.operation.as_str(), c.object.as_str(), c.detail.as_str()))
            .collect();
        assert_eq!(
            kids,
            [
                ("Table scan", "u", ""),
                ("Index lookup", "o", "using user_id (user_id=u.id)")
            ]
        );
        let table = parse_plan(
            &cols(&["id", "select_type", "table", "type", "key", "rows", "Extra"]),
            &[
                vec![
                    json!(1),
                    json!("SIMPLE"),
                    json!("u"),
                    json!("ALL"),
                    Value::Null,
                    json!(3),
                    Value::Null,
                ],
                vec![
                    json!(1),
                    json!("SIMPLE"),
                    json!("o"),
                    json!("ref"),
                    json!("user_id"),
                    json!("1"),
                    json!("Using where"),
                ],
            ],
        )
        .unwrap();
        assert_eq!(table.format, PlanFormat::MySqlTable);
        assert_eq!(table.root.children[0].operation, "Full table scan");
        assert_eq!(table.root.children[1].detail, "key: user_id; Using where");
        assert_eq!(table.weight, "rows");
        assert_eq!(table.flame[1].width, 0.75);
    }

    #[test]
    fn sqlite_query_plan() {
        let p = parse_plan(
            &cols(&["id", "parent", "notused", "detail"]),
            &[
                vec![json!(3), json!(0), json!(0), json!("SCAN o")],
                vec![
                    json!(5),
                    json!(0),
                    json!(0),
                    json!("SEARCH u USING INTEGER PRIMARY KEY (rowid=?)"),
                ],
                vec![
                    json!(9),
                    json!(0),
                    json!(0),
                    json!("USE TEMP B-TREE FOR ORDER BY"),
                ],
            ],
        )
        .unwrap();
        assert_eq!(p.format, PlanFormat::SqliteQueryPlan);
        assert_eq!(p.root.operation, "QUERY PLAN");
        let kids: Vec<(&str, &str, &str)> = p
            .root
            .children
            .iter()
            .map(|c| (c.operation.as_str(), c.object.as_str(), c.detail.as_str()))
            .collect();
        assert_eq!(
            kids,
            [
                ("SCAN", "o", ""),
                ("SEARCH", "u", "USING INTEGER PRIMARY KEY (rowid=?)"),
                ("USE TEMP B-TREE FOR ORDER BY", "", "")
            ]
        );
        assert_eq!(p.weight, "count");
        assert!((p.flame[1].width - 1.0 / 3.0).abs() < 1e-9);
        // Nested (subquery) rows.
        let nested = parse_plan(
            &cols(&["id", "parent", "notused", "detail"]),
            &[
                vec![json!(2), json!(0), json!(0), json!("SCAN users")],
                vec![
                    json!(4),
                    json!(0),
                    json!(0),
                    json!("CORRELATED SCALAR SUBQUERY 1"),
                ],
                vec![
                    json!(8),
                    json!(4),
                    json!(0),
                    json!("SEARCH orders USING INDEX orders_user_id_index (user_id=?)"),
                ],
            ],
        )
        .unwrap();
        assert_eq!(nested.root.children[1].children[0].object, "orders");
    }

    #[test]
    fn sqlserver_showplan_and_postgres_text() {
        let c = cols(&[
            "StmtText",
            "NodeId",
            "Parent",
            "PhysicalOp",
            "LogicalOp",
            "Argument",
            "EstimateRows",
            "TotalSubtreeCost",
        ]);
        let p = parse_plan(
            &c,
            &[
                vec![
                    json!("SELECT * FROM users"),
                    json!(1),
                    json!(0),
                    Value::Null,
                    Value::Null,
                    Value::Null,
                    json!(3),
                    json!(0.0033),
                ],
                vec![
                    json!("  |--Clustered Index Scan"),
                    json!(2),
                    json!(1),
                    json!("Clustered Index Scan"),
                    json!("Clustered Index Scan"),
                    json!("OBJECT:([shop].[dbo].[users].[PK_users])"),
                    json!(3),
                    json!(0.0033),
                ],
            ],
        )
        .unwrap();
        assert_eq!(p.format, PlanFormat::SqlServerShowplan);
        assert_eq!(p.root.operation, "SELECT * FROM users");
        assert_eq!(p.root.children[0].object, "users");
        assert_eq!(p.root.children[0].cost, Some(0.0033));
        let text = [
            "Seq Scan on users u  (cost=0.00..35.50 rows=2550 width=4)",
            "  Filter: (id > 1)",
        ];
        let p = parse_plan(
            &cols(&["QUERY PLAN"]),
            &text.iter().map(|t| vec![json!(t)]).collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(p.format, PlanFormat::Text);
        assert_eq!(
            (p.root.operation.as_str(), p.root.object.as_str()),
            ("Seq Scan", "users u")
        );
        assert_eq!(
            (p.root.startup_cost, p.root.cost, p.root.rows),
            (Some(0.0), Some(35.5), Some(2550.0))
        );
        assert_eq!(p.root.detail, "Filter: (id > 1)");
        assert!(parse_plan(&cols(&["x"]), &[]).is_err());
    }
}
