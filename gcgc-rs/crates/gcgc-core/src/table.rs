//! A small typed column store that replaces the pandas DataFrame used by the original tool.
//!
//! Every column is either numeric (`Option<f64>`) or text (`Option<String>`); `None` plays the role
//! of pandas' `None`/`NaN`. Integer columns from the log (e.g. `GCIndex`) are stored as `f64`, which is
//! also what pandas does once a missing value is present.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "values", rename_all = "lowercase")]
pub enum Column {
    Num(Vec<Option<f64>>),
    Str(Vec<Option<String>>),
}

impl Column {
    pub fn len(&self) -> usize {
        match self {
            Column::Num(v) => v.len(),
            Column::Str(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, row: usize) -> Cell<'_> {
        match self {
            Column::Num(v) => v[row].map_or(Cell::Null, Cell::Num),
            Column::Str(v) => v[row].as_deref().map_or(Cell::Null, Cell::Str),
        }
    }

    pub fn as_num(&self) -> Option<&[Option<f64>]> {
        match self {
            Column::Num(v) => Some(v),
            Column::Str(_) => None,
        }
    }

    pub fn as_str(&self) -> Option<&[Option<String>]> {
        match self {
            Column::Str(v) => Some(v),
            Column::Num(_) => None,
        }
    }

    fn take(&self, rows: &[usize]) -> Column {
        match self {
            Column::Num(v) => Column::Num(rows.iter().map(|&r| v[r]).collect()),
            Column::Str(v) => Column::Str(rows.iter().map(|&r| v[r].clone()).collect()),
        }
    }

    fn null_of_len(&self, n: usize) -> Column {
        match self {
            Column::Num(_) => Column::Num(vec![None; n]),
            Column::Str(_) => Column::Str(vec![None; n]),
        }
    }

    fn extend(&mut self, other: &Column) {
        match (self, other) {
            (Column::Num(a), Column::Num(b)) => a.extend_from_slice(b),
            (Column::Str(a), Column::Str(b)) => a.extend(b.iter().cloned()),
            // pandas would upcast to object; mixed numeric/text columns never occur in GC tables.
            (Column::Num(a), Column::Str(b)) => a.extend(b.iter().map(|_| None)),
            (Column::Str(a), Column::Num(b)) => a.extend(b.iter().map(|_| None)),
        }
    }
}

/// A borrowed cell value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cell<'a> {
    Null,
    Num(f64),
    Str(&'a str),
}

impl<'a> Cell<'a> {
    pub fn is_null(&self) -> bool {
        matches!(self, Cell::Null)
    }

    /// Python truthiness: `None`, `0.0`, `NaN`-free zero and `""` are false.
    /// (NaN is truthy in Python, but NaN never survives into a table here: it is stored as `None`.)
    pub fn truthy(&self) -> bool {
        match self {
            Cell::Null => false,
            Cell::Num(x) => *x != 0.0,
            Cell::Str(s) => !s.is_empty(),
        }
    }

    pub fn num(&self) -> Option<f64> {
        match self {
            Cell::Num(x) => Some(*x),
            _ => None,
        }
    }

    pub fn str(&self) -> Option<&'a str> {
        match self {
            Cell::Str(s) => Some(s),
            _ => None,
        }
    }
}

/// An ordered set of equally long, named columns.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Table {
    names: Vec<String>,
    columns: Vec<Column>,
    len: usize,
    /// True for tables whose numeric columns are numpy float64/int64 in the original (CSV input).
    /// Parsed GC tables are pandas `object` columns of Python floats. Only affects `round()`.
    #[serde(skip)]
    pub numpy: bool,
}

impl Table {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// pandas `df.empty`: true when there are no rows (or no columns).
    pub fn is_empty(&self) -> bool {
        self.len == 0 || self.columns.is_empty()
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn has(&self, name: &str) -> bool {
        self.names.iter().any(|n| n == name)
    }

    pub fn column(&self, name: &str) -> Option<&Column> {
        self.names.iter().position(|n| n == name).map(|i| &self.columns[i])
    }

    pub fn num(&self, name: &str) -> Option<&[Option<f64>]> {
        self.column(name).and_then(Column::as_num)
    }

    pub fn str(&self, name: &str) -> Option<&[Option<String>]> {
        self.column(name).and_then(Column::as_str)
    }

    pub fn cell(&self, name: &str, row: usize) -> Cell<'_> {
        self.column(name).map_or(Cell::Null, |c| c.get(row))
    }

    /// Insert or replace a column (pandas `df[name] = values`). Replacing keeps the column position.
    pub fn set(&mut self, name: &str, column: Column) {
        assert!(
            self.columns.is_empty() || column.len() == self.len,
            "column {name} has {} rows, table has {}",
            column.len(),
            self.len
        );
        self.len = column.len();
        match self.names.iter().position(|n| n == name) {
            Some(i) => self.columns[i] = column,
            None => {
                self.names.push(name.to_string());
                self.columns.push(column);
            }
        }
    }

    pub fn drop(&mut self, name: &str) {
        if let Some(i) = self.names.iter().position(|n| n == name) {
            self.names.remove(i);
            self.columns.remove(i);
        }
    }

    /// Select rows by index, in the given order.
    pub fn take(&self, rows: &[usize]) -> Table {
        Table {
            names: self.names.clone(),
            columns: self.columns.iter().map(|c| c.take(rows)).collect(),
            len: rows.len(),
            numpy: self.numpy,
        }
    }

    /// Keep rows where `keep(table, row)` is true.
    pub fn filter(&self, mut keep: impl FnMut(&Table, usize) -> bool) -> Table {
        let rows: Vec<usize> = (0..self.len).filter(|&r| keep(self, r)).collect();
        self.take(&rows)
    }

    /// pandas `concat` of tables: columns are unioned in first-seen order, missing cells are `None`.
    pub fn concat(tables: &[Table]) -> Table {
        let mut out = Table::new();
        for t in tables {
            for (name, col) in t.names.iter().zip(&t.columns) {
                if !out.has(name) {
                    let template = col.null_of_len(out.len);
                    out.names.push(name.clone());
                    out.columns.push(template);
                }
            }
            let start = out.len;
            for (i, name) in out.names.clone().iter().enumerate() {
                match t.column(name) {
                    Some(col) => out.columns[i].extend(col),
                    None => {
                        let nulls = out.columns[i].null_of_len(t.len);
                        out.columns[i].extend(&nulls);
                    }
                }
            }
            out.len = start + t.len;
        }
        out
    }
}
