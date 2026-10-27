//! DataFrame tables: pandas' HTML (`DataFrame._repr_html_`), the Table Schema of
//! `application/vnd.dataresource+json` and `DataFrame.to_json(orient="table")` (View as
//! DataFrame) into one [`DataFrame`] model, and its sorting (numeric-aware, through
//! `forge_viewers::sort_order`), filtering and pages ([`query`]).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::html::{self, Table};

/// A table of strings; the first `index_columns` columns are the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DataFrame {
    pub columns: Vec<String>,
    /// Column types when known (Table Schema: `integer`, `number`, `string`, …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dtypes: Vec<String>,
    #[serde(default)]
    pub index_columns: usize,
    pub rows: Vec<Vec<String>>,
    /// Size of the whole frame when the output shows part of it (pandas' "1000 rows × 5
    /// columns" footer, or the length of a variable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_rows: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_columns: Option<u64>,
}

/// A pandas DataFrame from its HTML; `None` for other HTML.
pub fn from_html(src: &str) -> Option<DataFrame> {
    if !src.contains("dataframe") {
        return None;
    }
    let table = html::tables(src)
        .into_iter()
        .find(|t| t.class.split_whitespace().any(|c| c == "dataframe"))?;
    Some(from_table(&table))
}

/// Expand colspan / rowspan into a grid of texts.
fn grid(rows: &[&html::TableRow]) -> Vec<Vec<(String, bool)>> {
    let mut out: Vec<Vec<Option<(String, bool)>>> = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        if out.len() <= r {
            out.resize(r + 1, Vec::new());
        }
        let mut c = 0;
        for cell in &row.cells {
            while out[r].get(c).is_some_and(Option::is_some) {
                c += 1;
            }
            for dr in 0..cell.rowspan.min(rows.len() - r) {
                if out.len() <= r + dr {
                    out.resize(r + dr + 1, Vec::new());
                }
                for dc in 0..cell.colspan {
                    let line = &mut out[r + dr];
                    if line.len() <= c + dc {
                        line.resize(c + dc + 1, None);
                    }
                    line[c + dc] = Some((cell.text.clone(), cell.header));
                }
            }
            c += cell.colspan;
        }
    }
    out.into_iter()
        .map(|r| r.into_iter().map(Option::unwrap_or_default).collect())
        .collect()
}

fn from_table(t: &Table) -> DataFrame {
    let head_rows: Vec<&html::TableRow> = t.rows.iter().filter(|r| r.head).collect();
    let body_rows: Vec<&html::TableRow> = t.rows.iter().filter(|r| !r.head).collect();
    let head = grid(&head_rows);
    let body = grid(&body_rows);
    let width = head
        .iter()
        .chain(body.iter())
        .map(Vec::len)
        .max()
        .unwrap_or(0);
    let index_columns = body
        .first()
        .map(|r| r.iter().take_while(|(_, h)| *h).count())
        .unwrap_or(0)
        .min(width);
    // pandas puts the index names in a header row of their own: names first, rest empty.
    let is_index_names = |r: &Vec<(String, bool)>| {
        index_columns > 0
            && r.iter().take(index_columns).any(|(t, _)| !t.is_empty())
            && r.iter().skip(index_columns).all(|(t, _)| t.is_empty())
    };
    let mut names: Vec<Vec<String>> = vec![Vec::new(); width];
    let mut index_names: Vec<String> = vec![String::new(); index_columns];
    for row in &head {
        if head.len() > 1 && is_index_names(row) {
            for (i, (t, _)) in row.iter().take(index_columns).enumerate() {
                index_names[i] = t.clone();
            }
            continue;
        }
        for (i, (t, _)) in row.iter().enumerate() {
            if !t.is_empty() && names[i].last() != Some(t) {
                names[i].push(t.clone());
            }
        }
    }
    let columns = (0..width)
        .map(|i| {
            if i < index_columns && !index_names[i].is_empty() {
                return index_names[i].clone();
            }
            let n = names[i].join(" / ");
            if n.is_empty() && i < index_columns {
                String::new()
            } else {
                n
            }
        })
        .collect();
    let rows = body
        .into_iter()
        .map(|r| {
            let mut v: Vec<String> = r.into_iter().map(|(t, _)| t).collect();
            v.resize(width, String::new());
            v
        })
        .collect();
    let (total_rows, total_columns) = shape(&t.after);
    DataFrame {
        columns,
        dtypes: Vec::new(),
        index_columns,
        rows,
        total_rows,
        total_columns,
    }
}

/// pandas' "1000 rows × 5 columns".
fn shape(text: &str) -> (Option<u64>, Option<u64>) {
    let words: Vec<&str> = text.split_whitespace().collect();
    let num = |w: &str| w.replace(',', "").parse::<u64>().ok();
    let mut rows = None;
    let mut cols = None;
    for (i, w) in words.iter().enumerate() {
        if i == 0 {
            continue;
        }
        if w.starts_with("row") {
            rows = num(words[i - 1]);
        } else if w.starts_with("column") {
            cols = num(words[i - 1]);
        }
    }
    (rows, cols)
}

fn cell_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// A frame from Table Schema JSON (`{ schema: { fields, primaryKey }, data: [...] }`).
pub fn from_dataresource(v: &Value) -> Option<DataFrame> {
    let fields = v.pointer("/schema/fields")?.as_array()?;
    let data = v.get("data")?.as_array()?;
    let names: Vec<String> = fields
        .iter()
        .map(|f| f.get("name").map(cell_text).unwrap_or_default())
        .collect();
    let dtypes = fields
        .iter()
        .map(|f| {
            f.get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        })
        .collect();
    let index_columns = v
        .pointer("/schema/primaryKey")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
        .min(names.len());
    let rows = data
        .iter()
        .map(|row| match row {
            Value::Object(m) => names
                .iter()
                .map(|n| m.get(n).map(cell_text).unwrap_or_default())
                .collect(),
            Value::Array(a) => (0..names.len())
                .map(|i| a.get(i).map(cell_text).unwrap_or_default())
                .collect(),
            _ => vec![String::new(); names.len()],
        })
        .collect();
    let total_rows = v.get("totalRows").and_then(Value::as_u64);
    Some(DataFrame {
        columns: names
            .into_iter()
            .enumerate()
            .map(|(i, n)| {
                if i < index_columns && n == "index" {
                    String::new()
                } else {
                    n
                }
            })
            .collect(),
        dtypes,
        index_columns,
        rows,
        total_rows,
        total_columns: None,
    })
}

/// Sorting, filtering and the page to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    #[serde(default)]
    pub sort: Option<SortBy>,
    /// Case-insensitive text every shown row contains in some cell; `column: text` limits
    /// it to one column.
    #[serde(default)]
    pub filter: String,
    #[serde(default)]
    pub page: usize,
    /// Rows per page; 0 shows all.
    #[serde(default)]
    pub page_size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortBy {
    pub column: usize,
    #[serde(default)]
    pub descending: bool,
}

/// The rows of one page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub rows: Vec<Vec<String>>,
    /// Position of each shown row in the frame.
    pub row_numbers: Vec<usize>,
    /// Rows that pass the filter.
    pub matched: usize,
    pub page: usize,
    pub pages: usize,
}

/// Sort, filter and cut a page of `frame`.
pub fn query(frame: &DataFrame, q: &Query) -> Page {
    let mut order: Vec<usize> = match &q.sort {
        Some(s) if s.column < frame.columns.len() => {
            forge_viewers::sort_order(&frame.rows, s.column, s.descending, 0)
        }
        _ => (0..frame.rows.len()).collect(),
    };
    let filter = q.filter.trim();
    if !filter.is_empty() {
        let (column, needle) = match filter.split_once(':') {
            Some((c, n)) => match frame
                .columns
                .iter()
                .position(|name| name.trim().eq_ignore_ascii_case(c.trim()))
            {
                Some(i) => (Some(i), n.trim()),
                None => (None, filter),
            },
            None => (None, filter),
        };
        let needle = needle.to_lowercase();
        order.retain(|&r| {
            let row = &frame.rows[r];
            match column {
                Some(c) => row
                    .get(c)
                    .is_some_and(|v| v.to_lowercase().contains(&needle)),
                None => row.iter().any(|v| v.to_lowercase().contains(&needle)),
            }
        });
    }
    let matched = order.len();
    let (page, pages, slice) = if q.page_size == 0 {
        (0, 1, &order[..])
    } else {
        let pages = matched.div_ceil(q.page_size).max(1);
        let page = q.page.min(pages - 1);
        let start = page * q.page_size;
        let end = (start + q.page_size).min(matched);
        (page, pages, &order[start.min(matched)..end])
    };
    Page {
        rows: slice.iter().map(|&r| frame.rows[r].clone()).collect(),
        row_numbers: slice.to_vec(),
        matched,
        page,
        pages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PANDAS: &str = r#"<div>
<style scoped>
    .dataframe tbody tr th:only-of-type { vertical-align: middle; }
</style>
<table border="1" class="dataframe">
  <thead>
    <tr style="text-align: right;">
      <th></th>
      <th>product</th>
      <th>price</th>
    </tr>
    <tr>
      <th>id</th>
      <th></th>
      <th></th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <th>1</th>
      <td>Pen</td>
      <td>10.5</td>
    </tr>
    <tr>
      <th>2</th>
      <td>Ink</td>
      <td>9</td>
    </tr>
    <tr>
      <th>3</th>
      <td>Paper &amp; Co</td>
      <td>100</td>
    </tr>
  </tbody>
</table>
<p>3 rows × 2 columns</p>
</div>"#;

    #[test]
    fn reads_pandas_html_with_index_names() {
        let f = from_html(PANDAS).unwrap();
        assert_eq!(f.columns, ["id", "product", "price"]);
        assert_eq!(f.index_columns, 1);
        assert_eq!(f.rows[2], ["3", "Paper & Co", "100"]);
        assert_eq!((f.total_rows, f.total_columns), (Some(3), Some(2)));
        assert!(from_html("<table><tr><td>x</td></tr></table>").is_none());
    }

    #[test]
    fn reads_multi_index_columns_and_rows() {
        let src = r#"<table class="dataframe"><thead>
<tr><th></th><th></th><th colspan="2" halign="left">sales</th></tr>
<tr><th></th><th></th><th>q1</th><th>q2</th></tr>
<tr><th>region</th><th>city</th><th></th><th></th></tr></thead>
<tbody><tr><th rowspan="2">EU</th><th>Berlin</th><td>1</td><td>2</td></tr>
<tr><th>Paris</th><td>3</td><td>4</td></tr></tbody></table>"#;
        let f = from_html(src).unwrap();
        assert_eq!(f.columns, ["region", "city", "sales / q1", "sales / q2"]);
        assert_eq!(f.index_columns, 2);
        assert_eq!(f.rows[1], ["EU", "Paris", "3", "4"]);
    }

    #[test]
    fn reads_table_schema() {
        let v = json!({
            "schema": {"fields": [{"name": "index", "type": "integer"}, {"name": "a", "type": "number"}, {"name": "b", "type": "string"}], "primaryKey": ["index"]},
            "data": [{"index": 0, "a": 1.5, "b": "x"}, {"index": 1, "a": null, "b": "y"}]
        });
        let f = from_dataresource(&v).unwrap();
        assert_eq!(f.columns, ["", "a", "b"]);
        assert_eq!(f.dtypes, ["integer", "number", "string"]);
        assert_eq!(f.rows, [["0", "1.5", "x"], ["1", "", "y"]]);
    }

    #[test]
    fn sorts_filters_and_pages() {
        let f = from_html(PANDAS).unwrap();
        let p = query(
            &f,
            &Query {
                sort: Some(SortBy {
                    column: 2,
                    descending: true,
                }),
                ..Query::default()
            },
        );
        assert_eq!(p.row_numbers, [2, 0, 1]);
        let p = query(
            &f,
            &Query {
                filter: "PEN".into(),
                ..Query::default()
            },
        );
        assert_eq!(p.row_numbers, [0]);
        let p = query(
            &f,
            &Query {
                filter: "product: in".into(),
                ..Query::default()
            },
        );
        assert_eq!(p.row_numbers, [1]);
        let p = query(
            &f,
            &Query {
                page: 5,
                page_size: 2,
                ..Query::default()
            },
        );
        assert_eq!(
            (p.page, p.pages, p.matched, p.row_numbers.clone()),
            (1, 2, 3, vec![2])
        );
    }
}
