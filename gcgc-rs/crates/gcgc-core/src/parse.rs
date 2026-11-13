//! Log line parsing: a port of `parse_log_file.py` (regex construction) and
//! `read_log_file.get_parsed_data_from_file` (regex → typed columns).

use crate::table::{Column, Table};
use fancy_regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DType {
    Str,
    Float,
    Int,
}

#[derive(Debug, Clone)]
struct Term {
    regex: String,
    column: Option<&'static str>,
    dtype: Option<DType>,
}

fn t(regex: &str, column: &'static str, dtype: DType) -> Term {
    Term { regex: regex.to_string(), column: Some(column), dtype: Some(dtype) }
}

fn bare(regex: &str) -> Term {
    Term { regex: regex.to_string(), column: None, dtype: None }
}

/// `__words(min, max)`: any `min..=max` words, each optionally followed by a space.
fn words(min: u32, max: u32) -> String {
    format!(r"(?:\w+ ?){{{min},{max}}}")
}

/// `__set_noncapture`: wrap a list of terms in one non-capturing group.
fn set_noncapture(mut terms: Vec<Term>) -> Vec<Term> {
    terms[0].regex = format!("(?:{}", terms[0].regex);
    let last = terms.len() - 1;
    terms[last].regex.push(')');
    terms
}

/// `__regex_or`: alternation between groups of terms, the whole wrapped in a non-capturing group.
fn regex_or(groups: Vec<Vec<Term>>) -> Vec<Term> {
    let mut groups: Vec<Vec<Term>> = groups.into_iter().map(set_noncapture).collect();
    for g in groups.iter_mut().skip(1) {
        g[0].regex = format!("|{}", g[0].regex);
    }
    groups[0][0].regex = format!("(?:{}", groups[0][0].regex);
    let lg = groups.len() - 1;
    let lt = groups[lg].len() - 1;
    groups[lg][lt].regex.push(')');
    groups.into_iter().flatten().collect()
}

/// The parsing groups: (regex string, column name per capture group, data type per capture group).
/// Mirrors `get_parsing_groups()` term for term so the generated regex is byte-identical.
pub fn parsing_groups() -> (String, Vec<&'static str>, Vec<DType>) {
    use DType::*;
    let start = bare("^");
    let date_time = t(r"\[(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}[+-]\d{4})\]", "DateTime", Str);
    let time = t(r"\[([\d\.]+)", "Time", Float);
    let time_unit = t(r"((?:s)|(?:ms)|(?:ns))\]", "TimeUnit", Str);
    let other_fields = t(r"((?:\[.*?\])*)", "Other fields", Str);
    let gc_phase = t(r" GC\((\d+)\)", "GCIndex", Int);
    let zgc_generation = t("( y:| Y:| o:| O:)?", "Generation", Str);
    let event_type = t(
        " ((?:Pause(?=.*ms))|(?:Concurrent(?=.*ms)|(?:Incremental GC)|(?:Full GC))|(?:.+ Collection)) ",
        "EventType",
        Str,
    );
    let event_name = t(&format!("(?:({}) )?", words(1, 4)), "EventName", Str);
    let additional_event_info = t(r"((?:\((?:\w+(?:\.gc\(\))? ?){1,3}\) ){0,3})", "AdditionalEventInfo", Str);
    let heap_before_gc = t(r"(?:([\d\.]+)M->", "HeapBeforeGC", Float);
    let heap_after_gc = t(r"([\d\.]+)M", "HeapAfterGC", Float);
    let max_heapsize = t(r"(?:\((\d+)M\)?)?)?(?=", "MaxHeapsize", Float);
    let duration_ms = t(r" ?(\d+\.\d+)ms)", "Duration_milliseconds", Float);
    let zgc_heap_before_gc = t(r"(\d+)M\(\d+%\)", "HeapBeforeGC", Float);
    let zgc_heap_after_gc = t(r"->(\d+)M", "HeapAfterGC", Float);
    let zgc_percent_full = t(r"\((\d+)%\)", "HeapPercentFull", Float);
    let safepoint_name = t(r#" Safepoint "(\w+)""#, "SafepointName", Str);
    let safepoint_time_since_last = t(r", Time since last: (\d+) ns, ", "TimeFromLastSafepoint_ns", Float);
    let safepoint_time_to_reach = t(r"Reaching safepoint: (\d+) ns, ", "TimeToReachSafepoint_ns", Float);
    let time_at_safepoint = t(r"At safepoint: (\d+) ns, ", "AtSafepoint_ns", Float);
    let total_time_safepoint = t(r"Total: (\d+) ns$", "TotalTimeAtSafepoint_ns", Float);
    let program_pause_time = t(
        r" Total time for which application threads were stopped: ([\d\.]+) seconds,",
        "TotalApplicationThreadPauseTime_seconds",
        Float,
    );
    let time_to_stop_application =
        t(r" Stopping threads took: ([\d\.]+) seconds$", "TimeToStopApplication_seconds", Float);
    let optional_gc_phase = bare(r"(?: GC\(\d+\))?");
    let used_metaspace = t(
        r" Metaspace: \d+[KMG]\(\d+[KMG]\)->(\d+[KMG])\(\d+[KMG]\) NonClass: \d+[KMG]\(\d+[KMG]\)->\d+[KMG]\(\d+[KMG]\) Class: \d+[KMG]\(\d+[KMG]\)->\d+[KMG]\(\d+[KMG]\)",
        "UsedMetaspaceAfterGCWithUnit",
        Str,
    );
    let zgc_used_metaspace =
        t(r" Metaspace: (\d+[KMG]) used, \d+[KMG] committed, \d+[KMG] reserved", "UsedMetaspaceAfterGCWithUnit", Str);
    let code_cache_flushing = t("( CodeCache flushing)", "CodeCacheFlushing", Str);
    let code_heap = t(" CodeHeap '(non-profiled nmethods|profiled nmethods|non-nmethods)': ", "CodeHeap", Str);
    let code_heap_size = t(r"size=(\d+)Kb ", "CodeHeapSize", Float);
    let code_heap_used = t(r"used=(\d+)Kb ", "CodeHeapUsed", Float);
    let code_heap_max_used = t(r"max_used=(\d+)Kb free=\d+Kb", "CodeHeapMaxUsed", Float);

    let mut gc_event = vec![gc_phase, zgc_generation, event_type, event_name, additional_event_info];
    gc_event.extend(regex_or(vec![
        vec![heap_before_gc, heap_after_gc, max_heapsize, duration_ms],
        vec![zgc_heap_before_gc, zgc_heap_after_gc, zgc_percent_full],
    ]));
    let mut metaspace = vec![optional_gc_phase];
    metaspace.extend(regex_or(vec![vec![used_metaspace], vec![zgc_used_metaspace]]));

    let mut combined = vec![start];
    combined.extend(regex_or(vec![vec![date_time], vec![time, time_unit]]));
    combined.push(other_fields);
    combined.extend(regex_or(vec![
        gc_event,
        vec![
            safepoint_name,
            safepoint_time_since_last,
            safepoint_time_to_reach,
            time_at_safepoint,
            total_time_safepoint,
        ],
        vec![program_pause_time, time_to_stop_application],
        metaspace,
        vec![code_cache_flushing],
        vec![code_heap, code_heap_size, code_heap_used, code_heap_max_used],
    ]));

    let regex: String = combined.iter().map(|t| t.regex.as_str()).collect();
    let columns = combined.iter().filter_map(|t| t.column).collect();
    let dtypes = combined.iter().filter_map(|t| t.dtype).collect();
    (regex, columns, dtypes)
}

struct Parser {
    regex: Regex,
    /// Unique column names in first-seen order, each with (capture group index, dtype) aliases.
    columns: Vec<(&'static str, Vec<(usize, DType)>)>,
}

fn parser() -> &'static Parser {
    static PARSER: OnceLock<Parser> = OnceLock::new();
    PARSER.get_or_init(|| {
        let (regex, names, dtypes) = parsing_groups();
        let mut columns: Vec<(&'static str, Vec<(usize, DType)>)> = Vec::new();
        for (group, (name, dtype)) in names.into_iter().zip(dtypes).enumerate() {
            match columns.iter_mut().find(|(n, _)| *n == name) {
                Some((_, aliases)) => aliases.push((group + 1, dtype)),
                None => columns.push((name, vec![(group + 1, dtype)])),
            }
        }
        Parser { regex: Regex::new(&regex).expect("GC log regex compiles"), columns }
    })
}

/// Every alternative after the timestamp prefix starts with one of these literals, so a line
/// containing none of them cannot match. Checking them first skips the (backtracking) regex for
/// most non-event lines without changing results.
const REQUIRED_LITERALS: [&str; 6] =
    [" GC(", " Safepoint \"", " Total time for which", " Metaspace: ", " CodeCache flushing", " CodeHeap '"];

#[inline]
fn may_match(line: &str) -> bool {
    REQUIRED_LITERALS.iter().any(|lit| line.contains(lit))
}

/// Split text into lines the way Python's text-mode file iteration does (universal newlines),
/// without the line terminators.
pub fn lines(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        match rest.find(['\n', '\r']) {
            None => {
                let line = rest;
                rest = "";
                Some(line)
            }
            Some(i) => {
                let line = &rest[..i];
                let skip = if rest[i..].starts_with("\r\n") { 2 } else { 1 };
                rest = &rest[i + skip..];
                Some(line)
            }
        }
    })
}

/// Raw capture groups for every matching line: `(line_number, groups[1..])`.
pub fn captures(text: &str) -> Vec<(usize, Vec<Option<String>>)> {
    let p = parser();
    let mut out = Vec::new();
    for (ln, line) in lines(text).enumerate() {
        if !may_match(line) {
            continue;
        }
        if let Ok(Some(c)) = p.regex.captures(line) {
            let groups = (1..c.len()).map(|i| c.get(i).map(|m| m.as_str().to_string())).collect();
            out.push((ln, groups));
        }
    }
    out
}

/// Port of `get_parsed_data_from_file`: one row per matching line, columns typed and coalesced.
/// Returns the table plus diagnostics (values that failed numeric conversion).
pub fn parse_log(text: &str) -> (Table, Vec<String>) {
    let p = parser();
    let mut diags = Vec::new();
    let mut builders: Vec<Column> = p
        .columns
        .iter()
        .map(|(_, aliases)| match aliases[0].1 {
            DType::Str => Column::Str(Vec::new()),
            DType::Float | DType::Int => Column::Num(Vec::new()),
        })
        .collect();

    for (ln, line) in lines(text).enumerate() {
        if !may_match(line) {
            continue;
        }
        let caps = match p.regex.captures(line) {
            Ok(Some(c)) => c,
            Ok(None) => continue,
            Err(e) => {
                diags.push(format!("line {}: regex error: {e}", ln + 1));
                continue;
            }
        };
        for ((name, aliases), col) in p.columns.iter().zip(builders.iter_mut()) {
            // First non-None alias wins (`__create_column`).
            let hit = aliases.iter().find_map(|&(g, dt)| caps.get(g).map(|m| (m.as_str(), dt)));
            match col {
                Column::Str(v) => v.push(hit.map(|(s, _)| s.to_string())),
                Column::Num(v) => v.push(hit.and_then(|(s, _)| match s.parse::<f64>() {
                    Ok(x) => Some(x),
                    Err(_) => {
                        diags.push(format!("line {}: cannot convert {s:?} for column {name}", ln + 1));
                        None
                    }
                })),
            }
        }
    }

    let mut table = Table::new();
    for ((name, _), col) in p.columns.iter().zip(builders) {
        table.set(name, col);
    }

    // Special case: rows with safepoint information get EventType "Safepoint" (Python truthiness).
    let n = table.len();
    let safepoint: Vec<bool> = (0..n)
        .map(|r| table.cell("SafepointName", r).truthy() || table.cell("TimeToStopApplication_seconds", r).truthy())
        .collect();
    if let Some(Column::Str(event_type)) = table.column("EventType").cloned() {
        let fixed = event_type
            .into_iter()
            .zip(safepoint)
            .map(|(e, sp)| if sp { Some("Safepoint".to_string()) } else { e })
            .collect();
        table.set("EventType", Column::Str(fixed));
    }
    (table, diags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_universal_newlines() {
        let v: Vec<&str> = lines("a\nb\r\nc\rd\n\ne").collect();
        assert_eq!(v, ["a", "b", "c", "d", "", "e"]);
        assert_eq!(lines("x\n").collect::<Vec<_>>(), ["x"]);
    }

    #[test]
    fn regex_has_thirty_groups() {
        let (_, cols, types) = parsing_groups();
        assert_eq!(cols.len(), 30);
        assert_eq!(types.len(), 30);
    }
}
