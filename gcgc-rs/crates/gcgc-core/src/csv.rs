//! Minimal CSV reader for external data overlays (`pd.read_csv` in the original notebook).
//! Columns where every non-empty cell parses as a number become numeric; others are text.

use crate::table::{Column, Table};

fn split_record(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => fields.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    fields.push(cur);
    fields
}

/// Parse CSV text with a header row into a table.
pub fn read_csv(text: &str) -> Result<Table, String> {
    let mut lines = crate::parse::lines(text).filter(|l| !l.trim().is_empty());
    let header = split_record(lines.next().ok_or("CSV is empty")?);
    let rows: Vec<Vec<String>> = lines.map(split_record).collect();
    let mut t = Table::new();
    for (i, name) in header.iter().enumerate() {
        let cells: Vec<Option<&str>> =
            rows.iter().map(|r| r.get(i).map(|s| s.trim()).filter(|s| !s.is_empty())).collect();
        let numeric = cells.iter().flatten().all(|s| s.parse::<f64>().is_ok());
        let col = if numeric {
            Column::Num(cells.iter().map(|c| c.and_then(|s| s.parse().ok())).collect())
        } else {
            Column::Str(cells.iter().map(|c| c.map(str::to_string)).collect())
        };
        t.set(name.trim(), col);
    }
    t.numpy = true;
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_numeric_and_text() {
        let t = read_csv("Time Seconds,Reads Completed,Note\n10,14,a\n20,15,\"b, c\"\n").unwrap();
        assert_eq!(t.num("Time Seconds").unwrap(), &[Some(10.0), Some(20.0)]);
        assert_eq!(t.str("Note").unwrap()[1].as_deref(), Some("b, c"));
        assert!(t.numpy);
    }
}
