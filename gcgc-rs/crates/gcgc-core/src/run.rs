//! Building one "run" (one or more log files analysed together) into an event table.
//! Port of `read_log_file.get_gc_event_tables` and its helpers.

use crate::out::Out;
use crate::parse::parse_log;
use crate::table::{Column, Table};

/// One input file: a display name and its text.
#[derive(Debug, Clone)]
pub struct LogFile {
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy)]
pub struct RunOptions {
    /// Shift every run so its earliest event is at t = 0 (`zero_times`).
    pub zero_times: bool,
    /// Treat time going backwards (a JVM restart) as continuous runtime (`ignore_crashes`).
    /// The Python version of this option crashes; this implements its documented intent.
    pub ignore_crashes: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self { zero_times: true, ignore_crashes: false }
    }
}

/// A labelled run: the merged table of all its files.
#[derive(Debug, Clone)]
pub struct Run {
    pub label: String,
    pub files: Vec<String>,
    pub table: Table,
}

/// Port of `get_gc_event_tables` for a single run. Files are processed in the order given
/// (callers should sort wildcard matches by name). Messages the original prints go to `out`.
pub fn build_run(label: &str, files: &[LogFile], opts: RunOptions, out: &mut Out) -> Run {
    if opts.ignore_crashes {
        out.println("Warning: ignore_crashes takes log files and ignores all crashes.");
    }
    let mut tables = Vec::new();
    for f in files {
        let (mut t, diags) = parse_log(&f.text);
        for d in diags {
            out.diag(format!("{}: {d}", f.name));
        }
        scale_time(&mut t, out);
        if opts.ignore_crashes && !no_timing_errors(&t) {
            fix_timing_errors(&mut t);
        }
        scale_heap_percent_full(&mut t);
        metaspace_in_mb(&mut t, out);
        code_heap_percentages(&mut t);
        if t.is_empty() {
            out.println(&format!("No information collected for file:  {}", f.name));
        } else {
            tables.push(t);
        }
    }
    let mut table = Table::concat(&tables);
    if !tables.is_empty() && opts.zero_times {
        zero_start_times(&mut table);
    }
    Run { label: label.to_string(), files: files.iter().map(|f| f.name.clone()).collect(), table }
}

/// `scale_time`: TimeFromStart_seconds from Time + TimeUnit (unit taken from the first row), or
/// from DateTime when the first row has no unit. Drops Time and TimeUnit.
pub fn scale_time(t: &mut Table, out: &mut Out) {
    if t.is_empty() || !t.has("Time") || !t.has("TimeUnit") {
        return;
    }
    let unit = t.cell("TimeUnit", 0).str().map(str::to_string);
    let seconds: Vec<Option<f64>> = match unit.as_deref() {
        Some("s") | Some("ms") | Some("ns") => {
            let divisor = match unit.as_deref() {
                Some("s") => 1.0,
                Some("ms") => 1000.0,
                _ => 1_000_000_000.0,
            };
            t.num("Time").unwrap().iter().map(|v| v.map(|x| x / divisor)).collect()
        }
        None | Some("") => t
            .str("DateTime")
            .map(|c| c.iter().map(|v| v.as_deref().and_then(datetime_to_seconds)).collect())
            .unwrap_or_else(|| vec![None; t.len()]),
        Some(other) => {
            out.println(&format!("Unknown unit detected: unit =  {other}"));
            return;
        }
    };
    t.set("TimeFromStart_seconds", Column::Num(seconds));
    t.drop("Time");
    t.drop("TimeUnit");
}

/// Parse `2021-08-26T14:42:00.565-0400` into seconds since the Unix epoch (UTC).
/// The Python version relies on matplotlib's date2num (days × 86400); only differences matter.
pub fn datetime_to_seconds(s: &str) -> Option<f64> {
    let b = s.as_bytes();
    if b.len() != 28 {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec, ms) = (num(11..13)?, num(14..16)?, num(17..19)?, num(20..23)?);
    let sign = match b[23] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let (oh, om) = (num(24..26)?, num(26..28)?);
    // days from civil (Howard Hinnant's algorithm)
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let local = days * 86400 + h * 3600 + mi * 60 + sec;
    let utc = local - sign * (oh * 3600 + om * 60);
    Some(utc as f64 + ms as f64 / 1000.0)
}

/// `scale_heap_percent_full`. ZGC/GenZGC report the percentage directly (keep it only on major
/// "Collection" rows); for other collectors compute `100 * HeapAfterGC / MaxHeapsize` on
/// non-young collections.
pub fn scale_heap_percent_full(t: &mut Table) {
    if t.is_empty() {
        return;
    }
    let n = t.len();
    let event_type = t.str("EventType").map(<[_]>::to_vec).unwrap_or_else(|| vec![None; n]);
    if let Some(hpf) = t.num("HeapPercentFull") {
        // pandas stores missing values as NaN (truthy) once any value is present, so the
        // "prepopulated" branch triggers whenever the column has a value.
        let any_value = hpf.iter().any(Option::is_some);
        let triggered = any_value && hpf.iter().any(|v| v.is_none_or(|x| x != 0.0));
        if triggered {
            let fixed = hpf
                .iter()
                .zip(&event_type)
                .map(|(v, e)| match e {
                    Some(e) if e.contains("Collection") && !e.contains("Minor") => *v,
                    _ => None,
                })
                .collect();
            t.set("HeapPercentFull", Column::Num(fixed));
            return;
        }
    }
    let (Some(after), Some(max)) = (t.num("HeapAfterGC"), t.num("MaxHeapsize")) else {
        return;
    };
    let event_name = t.str("EventName").map(<[_]>::to_vec).unwrap_or_else(|| vec![None; n]);
    let info = t.str("AdditionalEventInfo").map(<[_]>::to_vec).unwrap_or_else(|| vec![None; n]);
    const MINOR_TAGS: [&str; 2] = ["Young", "Incremental"];
    let percent: Vec<Option<f64>> = (0..n)
        .map(|r| {
            let (Some(a), Some(m)) = (after[r], max[r]) else { return None };
            if m <= 0.0 {
                return None;
            }
            let mixed = info[r].as_deref() == Some("Mixed");
            let major = event_type[r].as_deref().is_some_and(|e| {
                !MINOR_TAGS
                    .iter()
                    .any(|tag| e.contains(tag) || event_name[r].as_deref().is_some_and(|nm| nm.contains(tag)))
            });
            (mixed || major).then(|| 100.0 * a / m)
        })
        .collect();
    t.set("HeapPercentFull", Column::Num(percent));
}

/// `generate_new_column_with_values_in_mb("UsedMetaspaceAfterGCWithUnit", "UsedMetaspaceAfterGC")`.
/// (The original intends to drop the base column but discards the result, so it is kept.)
pub fn metaspace_in_mb(t: &mut Table, out: &mut Out) {
    let Some(base) = t.str("UsedMetaspaceAfterGCWithUnit") else { return };
    let mut mb = Vec::with_capacity(base.len());
    for v in base {
        mb.push(v.as_deref().and_then(|s| {
            let (num, unit) = s.split_at(s.len().saturating_sub(1));
            let x: f64 = num.parse().ok()?;
            match unit {
                "K" => Some(x / 1024.0),
                "G" => Some(x * 1024.0),
                "M" => Some(x),
                other => {
                    out.println(&format!("Unknown unit detected when converting to MB: unit =  {other}"));
                    None
                }
            }
        }));
    }
    t.set("UsedMetaspaceAfterGC", Column::Num(mb));
}

pub const CODE_HEAP_COLUMNS: [(&str, &str, &str); 3] = [
    ("non-profiled nmethods", "NonProfiledNMethodsCodeHeapPercentUsed", "NonProfiledNMethodsCodeHeapPercentMaxUsed"),
    ("profiled nmethods", "ProfiledNMethodsCodeHeapPercentUsed", "ProfiledNMethodsCodeHeapPercentMaxUsed"),
    ("non-nmethods", "NonNMethodsCodeHeapPercentUsed", "NonNMethodsCodeHeapPercentMaxUsed"),
];

/// `generate_columns_for_percent_used_and_max_used_in_each_code_heap`: six percentage columns.
pub fn code_heap_percentages(t: &mut Table) {
    let n = t.len();
    let mut used = vec![vec![None; n]; 3];
    let mut max_used = vec![vec![None; n]; 3];
    if let (Some(heap), Some(size), Some(u), Some(mu)) =
        (t.str("CodeHeap"), t.num("CodeHeapSize"), t.num("CodeHeapUsed"), t.num("CodeHeapMaxUsed"))
    {
        for r in 0..n {
            let Some(h) = heap[r].as_deref() else { continue };
            let Some(k) = CODE_HEAP_COLUMNS.iter().position(|(name, _, _)| *name == h) else { continue };
            let s = size[r].unwrap_or(f64::NAN);
            let (pu, pm) = if s > 0.0 {
                (100.0 * u[r].unwrap_or(f64::NAN) / s, 100.0 * mu[r].unwrap_or(f64::NAN) / s)
            } else {
                (0.0, 0.0)
            };
            used[k][r] = Some(pu).filter(|x| !x.is_nan());
            max_used[k][r] = Some(pm).filter(|x| !x.is_nan());
        }
    }
    for (k, (_, used_name, _)) in CODE_HEAP_COLUMNS.iter().enumerate() {
        t.set(used_name, Column::Num(std::mem::take(&mut used[k])));
    }
    for (k, (_, _, max_name)) in CODE_HEAP_COLUMNS.iter().enumerate() {
        t.set(max_name, Column::Num(std::mem::take(&mut max_used[k])));
    }
}

/// `assert_no_timing_errors`: true if TimeFromStart_seconds never decreases.
pub fn no_timing_errors(t: &Table) -> bool {
    let Some(times) = t.num("TimeFromStart_seconds") else { return true };
    let mut max = -1.0;
    for x in times.iter().flatten() {
        if *x < max {
            return false;
        }
        max = *x;
    }
    true
}

/// `fix_timing_errors`: after a restart, continue the timeline from the last time seen.
pub fn fix_timing_errors(t: &mut Table) {
    let Some(times) = t.num("TimeFromStart_seconds") else { return };
    let mut max = 0.0;
    let mut add = 0.0;
    let fixed = times
        .iter()
        .map(|v| {
            v.map(|x| {
                if x + add < max {
                    add = max;
                }
                max = x + add;
                x + add
            })
        })
        .collect();
    t.set("TimeFromStart_seconds", Column::Num(fixed));
}

/// `zero_start_times`: subtract the earliest time.
pub fn zero_start_times(t: &mut Table) {
    let Some(times) = t.num("TimeFromStart_seconds") else { return };
    let min = times.iter().flatten().copied().fold(f64::INFINITY, f64::min);
    if !min.is_finite() {
        return;
    }
    let shifted = times.iter().map(|v| v.map(|x| x - min)).collect();
    t.set("TimeFromStart_seconds", Column::Num(shifted));
}

/// Sort a run's rows by time exactly like the notebook's `df.sort_values(by="TimeFromStart_seconds")`.
pub fn sort_by_time(t: &Table) -> Table {
    match t.num("TimeFromStart_seconds") {
        Some(times) => t.take(&crate::npsort::pandas_sort_order(times)),
        None => t.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datetime_parses() {
        let a = datetime_to_seconds("2021-08-26T14:42:00.565-0400").unwrap();
        let b = datetime_to_seconds("2021-08-26T14:42:59.565-0400").unwrap();
        assert!((b - a - 59.0).abs() < 1e-9);
        assert_eq!(datetime_to_seconds("1970-01-01T00:00:00.000+0000"), Some(0.0));
    }
}
