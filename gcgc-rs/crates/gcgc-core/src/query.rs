//! Row filters and grouping: a port of `filter_and_group.py`.
//!
//! The original accepted arbitrary Python predicates. Every predicate used by the notebook is one of
//! a few shapes, so filters are data here (and can be sent from the web UI as JSON).

use crate::chart::{colors_and_alphas, Color};
use crate::out::Out;
use crate::pyfmt::repr_f64;
use crate::table::{Cell, Table};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Num(f64),
    Str(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Filter {
    /// Every row.
    All,
    /// `row[column] == value` (false when the column is missing).
    Eq {
        column: String,
        value: Value,
    },
    /// `row[column] in values`.
    In {
        column: String,
        values: Vec<Value>,
    },
    /// `row[column] != None`.
    NotNull {
        column: String,
    },
    /// `if row[column]:` (Python truthiness: excludes None, 0 and "").
    Truthy {
        column: String,
    },
    /// `value in row[column]` for text columns.
    Contains {
        column: String,
        value: String,
    },
    And {
        filters: Vec<Filter>,
    },
    Or {
        filters: Vec<Filter>,
    },
    Not {
        filter: Box<Filter>,
    },
}

impl Filter {
    pub fn eq(column: &str, value: &str) -> Self {
        Filter::Eq { column: column.into(), value: Value::Str(value.into()) }
    }
    pub fn not_null(column: &str) -> Self {
        Filter::NotNull { column: column.into() }
    }
    pub fn truthy(column: &str) -> Self {
        Filter::Truthy { column: column.into() }
    }
    pub fn one_of(column: &str, values: &[&str]) -> Self {
        Filter::In { column: column.into(), values: values.iter().map(|v| Value::Str((*v).into())).collect() }
    }
    pub fn and(filters: Vec<Filter>) -> Self {
        Filter::And { filters }
    }

    pub fn matches(&self, t: &Table, row: usize) -> bool {
        fn eq(cell: Cell, v: &Value) -> bool {
            match (cell, v) {
                (Cell::Num(a), Value::Num(b)) => a == *b,
                (Cell::Str(a), Value::Str(b)) => a == b,
                _ => false,
            }
        }
        match self {
            Filter::All => true,
            Filter::Eq { column, value } => t.has(column) && eq(t.cell(column, row), value),
            Filter::In { column, values } => t.has(column) && values.iter().any(|v| eq(t.cell(column, row), v)),
            Filter::NotNull { column } => t.has(column) && !t.cell(column, row).is_null(),
            Filter::Truthy { column } => t.has(column) && t.cell(column, row).truthy(),
            Filter::Contains { column, value } => t.cell(column, row).str().is_some_and(|s| s.contains(value.as_str())),
            Filter::And { filters } => filters.iter().all(|f| f.matches(t, row)),
            Filter::Or { filters } => filters.iter().any(|f| f.matches(t, row)),
            Filter::Not { filter } => !filter.matches(t, row),
        }
    }
}

/// `apply_filter`: filtered copies of each table.
pub fn apply_filter(tables: &[&Table], filter: Option<&Filter>) -> Vec<Table> {
    tables
        .iter()
        .map(|t| match filter {
            Some(f) => {
                let mut out = t.filter(|t, r| f.matches(t, r));
                out.numpy = t.numpy;
                out
            }
            None => (*t).clone(),
        })
        .collect()
}

/// Output of `filter_and_group`: one (x, y) series per group.
#[derive(Debug, Clone)]
pub struct Groups {
    pub x: Vec<Vec<Option<f64>>>,
    pub y: Vec<Vec<Option<f64>>>,
    pub labels: Vec<String>,
    pub colors: Vec<Color>,
    pub alphas: Vec<f64>,
    /// True when the series are numpy float64 arrays in the original (grouped series, or tables
    /// read from CSV). This matters only for rounding: `round(np.float64)` differs from `round(float)`.
    pub numpy: bool,
}

/// Parameters shared by most plotting functions.
#[derive(Debug, Clone, Default)]
pub struct Select<'a> {
    pub group_by: Option<&'a str>,
    pub filter: Option<&'a Filter>,
    pub labels: Option<Vec<String>>,
    pub column: &'a str,
    pub colors: Option<Vec<Color>>,
    pub column_timing: Option<&'a str>,
}

/// Port of `filter_and_group`.
pub fn filter_and_group(tables: &[&Table], sel: &Select, out: &mut Out) -> Groups {
    let filtered = apply_filter(tables, sel.filter);
    let labels: Vec<String> = match &sel.labels {
        Some(l) if !l.is_empty() => l.clone(),
        _ => (1..=filtered.len()).map(|n| n.to_string()).collect(),
    };
    let timing = sel.column_timing.unwrap_or("TimeFromStart_seconds");
    let mut g = match sel.group_by {
        Some(group_by) => arrange_into_groups(&filtered, group_by, sel.column, timing, &labels, out),
        None => arrange_no_groups(&filtered, sel.column, timing, &labels),
    };
    match &sel.colors {
        Some(c) if !c.is_empty() => {
            g.alphas = vec![1.0; c.len()];
            g.colors = c.clone();
        }
        _ => {
            let (c, a) = colors_and_alphas(g.labels.len());
            g.colors = c;
            g.alphas = a;
        }
    }
    g
}

fn numeric(t: &Table, column: &str, rows: impl Iterator<Item = usize>) -> Vec<Option<f64>> {
    rows.map(|r| t.cell(column, r).num()).collect()
}

fn columns_repr(t: &Table) -> String {
    let names: Vec<String> = t.names().iter().map(|n| format!("'{n}'")).collect();
    format!("Index([{}], dtype='object')", names.join(", "))
}

/// `arrange_no_groups`: one series per non-empty table that has both columns.
fn arrange_no_groups(tables: &[Table], column: &str, timing: &str, labels: &[String]) -> Groups {
    let mut g = Groups { x: vec![], y: vec![], labels: vec![], colors: vec![], alphas: vec![], numpy: false };
    for (idx, t) in tables.iter().enumerate() {
        if !t.is_empty() && t.has(timing) && t.has(column) {
            g.x.push(numeric(t, timing, 0..t.len()));
            g.y.push(numeric(t, column, 0..t.len()));
            g.labels.push(labels.get(idx).cloned().unwrap_or_default());
            g.numpy |= t.numpy;
        }
    }
    g
}

#[derive(Debug, Clone, PartialEq)]
enum Key {
    Num(f64),
    Str(String),
}

impl Key {
    fn to_py_str(&self) -> String {
        match self {
            Key::Num(x) => repr_f64(*x),
            Key::Str(s) => s.clone(),
        }
    }
}

/// `arrange_into_groups`: per table, one series per distinct value of `group_by`, keys sorted.
fn arrange_into_groups(
    tables: &[Table],
    group_by: &str,
    column: &str,
    timing: &str,
    labels: &[String],
    out: &mut Out,
) -> Groups {
    let mut g = Groups { x: vec![], y: vec![], labels: vec![], colors: vec![], alphas: vec![], numpy: true };
    for (idx, t) in tables.iter().enumerate() {
        if t.is_empty() {
            continue;
        }
        if !t.has(group_by) {
            out.println(&format!(
                "Warning: group_by group {group_by} column not in dataset with columns {}",
                columns_repr(t)
            ));
            continue;
        }
        if !t.has(column) {
            out.println(&format!("Warning: column \"{column}\" not in dataset with columns {}", columns_repr(t)));
            continue;
        }
        if !t.has(timing) {
            out.println(&format!(
                "Warning: column_timing \"{timing}\" not in dataset with columns {}",
                columns_repr(t)
            ));
            continue;
        }
        // Insertion-ordered dict of groups.
        let mut groups: Vec<(Key, Vec<usize>)> = Vec::new();
        for r in 0..t.len() {
            let cell = t.cell(group_by, r);
            let key = if !cell.truthy() {
                Key::Str(format!("( {group_by} = None )"))
            } else {
                match cell {
                    Cell::Num(x) => Key::Num(x),
                    Cell::Str(s) => Key::Str(s.to_string()),
                    Cell::Null => unreachable!("null is falsy"),
                }
            };
            match groups.iter_mut().find(|(k, _)| *k == key) {
                Some((_, rows)) => rows.push(r),
                None => groups.push((key, vec![r])),
            }
        }
        // `keys.sort()`: numbers sort numerically, strings by code point. (Python raises on a mix;
        // we put numbers first.)
        groups.sort_by(|(a, _), (b, _)| match (a, b) {
            (Key::Num(x), Key::Num(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
            (Key::Str(x), Key::Str(y)) => x.cmp(y),
            (Key::Num(_), Key::Str(_)) => std::cmp::Ordering::Less,
            (Key::Str(_), Key::Num(_)) => std::cmp::Ordering::Greater,
        });
        let label = labels.get(idx).cloned().unwrap_or_default();
        for (key, rows) in groups {
            g.x.push(numeric(t, timing, rows.iter().copied()));
            g.y.push(numeric(t, column, rows.iter().copied()));
            g.labels.push(format!("{label}: {}", key.to_py_str()));
        }
    }
    g
}
