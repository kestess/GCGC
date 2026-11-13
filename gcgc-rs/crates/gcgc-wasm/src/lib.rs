//! WebAssembly bindings: a `Session` holding labelled runs, used by the web front end.
//! All structured data crosses the boundary as JSON strings.

use gcgc_core::analyses::{self, Ctx, External, Params};
use gcgc_core::csv::read_csv;
use gcgc_core::run::sort_by_time;
use gcgc_core::{build_run, Column, LogFile, Out, Run, RunOptions, Table};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct ColumnInfo {
    name: String,
    numeric: bool,
    non_null: usize,
}

#[derive(Serialize)]
struct RunSummary {
    label: String,
    files: Vec<String>,
    rows: usize,
    pauses: usize,
    concurrent: usize,
    first_time: Option<f64>,
    last_time: Option<f64>,
    columns: Vec<ColumnInfo>,
    stdout: String,
    diagnostics: Vec<String>,
}

fn summarize(run: &Run, out: &Out) -> RunSummary {
    let t = &run.table;
    let count = |v: &str| t.str("EventType").map_or(0, |c| c.iter().filter(|e| e.as_deref() == Some(v)).count());
    let times: Vec<f64> =
        t.num("TimeFromStart_seconds").map(|c| c.iter().flatten().copied().collect()).unwrap_or_default();
    RunSummary {
        label: run.label.clone(),
        files: run.files.clone(),
        rows: t.len(),
        pauses: count("Pause"),
        concurrent: count("Concurrent"),
        first_time: times.iter().copied().reduce(f64::min),
        last_time: times.iter().copied().reduce(f64::max),
        columns: t
            .names()
            .iter()
            .map(|n| {
                let c = t.column(n).unwrap();
                let non_null = match c {
                    Column::Num(v) => v.iter().filter(|x| x.is_some()).count(),
                    Column::Str(v) => v.iter().filter(|x| x.is_some()).count(),
                };
                ColumnInfo { name: n.clone(), numeric: matches!(c, Column::Num(_)), non_null }
            })
            .collect(),
        stdout: out.stdout.clone(),
        diagnostics: out.diagnostics.clone(),
    }
}

fn to_json<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|e| format!("{{\"error\":{:?}}}", e.to_string()))
}

#[wasm_bindgen]
#[derive(Default)]
pub struct Session {
    runs: Vec<Run>,
    sorted: Vec<Table>,
    external: Option<External>,
}

#[wasm_bindgen]
impl Session {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Session {
        Session::default()
    }

    /// Parse one run from one or more files (sorted by name, like a wildcard match).
    /// Returns a JSON run summary.
    #[wasm_bindgen(js_name = addRun)]
    pub fn add_run(
        &mut self,
        label: String,
        names: Vec<String>,
        texts: Vec<String>,
        zero_times: bool,
        ignore_crashes: bool,
    ) -> String {
        let mut files: Vec<LogFile> = names.into_iter().zip(texts).map(|(name, text)| LogFile { name, text }).collect();
        files.sort_by(|a, b| a.name.cmp(&b.name));
        let mut out = Out::new();
        let run = build_run(&label, &files, RunOptions { zero_times, ignore_crashes }, &mut out);
        let summary = summarize(&run, &out);
        self.sorted.push(sort_by_time(&run.table));
        self.runs.push(run);
        to_json(&summary)
    }

    #[wasm_bindgen(js_name = removeRun)]
    pub fn remove_run(&mut self, index: usize) {
        if index < self.runs.len() {
            self.runs.remove(index);
            self.sorted.remove(index);
        }
    }

    #[wasm_bindgen(js_name = renameRun)]
    pub fn rename_run(&mut self, index: usize, label: String) {
        if let Some(r) = self.runs.get_mut(index) {
            r.label = label;
        }
    }

    /// Move a run from one position to another (run order sets colours and legend order).
    #[wasm_bindgen(js_name = moveRun)]
    pub fn move_run(&mut self, from: usize, to: usize) {
        if from < self.runs.len() && to < self.runs.len() {
            let r = self.runs.remove(from);
            self.runs.insert(to, r);
            let s = self.sorted.remove(from);
            self.sorted.insert(to, s);
        }
    }

    pub fn clear(&mut self) {
        *self = Session::default();
    }

    #[wasm_bindgen(js_name = runCount)]
    pub fn run_count(&self) -> usize {
        self.runs.len()
    }

    /// Load a CSV for the external-data overlay. Returns JSON `{columns: [...]}` or `{error}`.
    #[wasm_bindgen(js_name = setExternal)]
    pub fn set_external(&mut self, label: String, csv_text: String, x_column: String, y_column: String) -> String {
        match read_csv(&csv_text) {
            Ok(table) => {
                let columns: Vec<String> = table.names().to_vec();
                self.external = Some(External { label, table, x_column, y_column });
                to_json(&serde_json::json!({ "columns": columns }))
            }
            Err(e) => to_json(&serde_json::json!({ "error": e })),
        }
    }

    #[wasm_bindgen(js_name = clearExternal)]
    pub fn clear_external(&mut self) {
        self.external = None;
    }

    /// JSON list of analyses with their parameters and defaults.
    pub fn catalogue() -> String {
        to_json(&analyses::catalogue())
    }

    /// Run an analysis. `params_json` may be "" or "{}" for notebook defaults.
    /// Returns JSON `{figures, stdout, diagnostics}` or `{error}`.
    pub fn analyse(&self, id: &str, params_json: &str) -> String {
        let params: Params = if params_json.trim().is_empty() {
            Params::default()
        } else {
            match serde_json::from_str(params_json) {
                Ok(p) => p,
                Err(e) => return to_json(&serde_json::json!({ "error": format!("bad parameters: {e}") })),
            }
        };
        if self.runs.is_empty() {
            return to_json(&serde_json::json!({ "error": "no runs loaded" }));
        }
        let ctx = Ctx {
            labels: self.runs.iter().map(|r| r.label.clone()).collect(),
            loaded: self.runs.iter().map(|r| &r.table).collect(),
            sorted: self.sorted.iter().collect(),
            external: self.external.as_ref(),
        };
        match analyses::run(id, &ctx, &params) {
            Ok(o) => to_json(&o),
            Err(e) => to_json(&serde_json::json!({ "error": e })),
        }
    }

    /// Export one run's event table as CSV.
    #[wasm_bindgen(js_name = exportCsv)]
    pub fn export_csv(&self, index: usize) -> String {
        let Some(run) = self.runs.get(index) else { return String::new() };
        table_to_csv(&run.table)
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn table_to_csv(t: &Table) -> String {
    let mut out = t.names().iter().map(|n| csv_field(n)).collect::<Vec<_>>().join(",");
    out.push('\n');
    for r in 0..t.len() {
        let row: Vec<String> = t
            .names()
            .iter()
            .map(|n| match t.cell(n, r) {
                gcgc_core::Cell::Null => String::new(),
                gcgc_core::Cell::Num(x) => gcgc_core::pyfmt::repr_f64(x),
                gcgc_core::Cell::Str(s) => csv_field(s),
            })
            .collect();
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_round_trip() {
        let mut s = Session::new();
        let log = "[0.010s][info][gc] GC(0) Pause Young (Normal) (G1 Evacuation Pause) 24M->4M(256M) 3.123ms\n\
                   [0.020s][info][gc] GC(1) Pause Young (Normal) (G1 Evacuation Pause) 28M->5M(256M) 2.000ms\n";
        let summary: serde_json::Value =
            serde_json::from_str(&s.add_run("G1".into(), vec!["a.log".into()], vec![log.into()], true, false)).unwrap();
        assert_eq!(summary["rows"], 2);
        assert_eq!(summary["pauses"], 2);
        let out: serde_json::Value = serde_json::from_str(&s.analyse("stw_pauses", "")).unwrap();
        assert_eq!(out["figures"][0]["axes"]["calls"][0]["y"], serde_json::json!([3.123, 2.0]));
        let cat: serde_json::Value = serde_json::from_str(&Session::catalogue()).unwrap();
        assert!(cat.as_array().unwrap().len() >= 23);
        assert!(s.export_csv(0).starts_with("DateTime,Other fields,GCIndex"));
        for kind in gcgc_core::analyses::CUSTOM_KINDS {
            let params = format!(r#"{{"kind":"{kind}","group_by":"EventName"}}"#);
            let out: serde_json::Value = serde_json::from_str(&s.analyse("custom", &params)).unwrap();
            assert!(out.get("error").is_none(), "{kind}: {out}");
            assert!(out["figures"].as_array().unwrap().len() == 1 || !out["stdout"].as_str().unwrap().is_empty(), "{kind}");
        }
        let all: serde_json::Value = serde_json::from_str(&s.analyse("custom", r#"{"filter":{"op":"all"}}"#)).unwrap();
        assert!(all.get("error").is_none());
        let bad: serde_json::Value = serde_json::from_str(&s.analyse("nope", "")).unwrap();
        assert!(bad["error"].is_string());
    }
}
