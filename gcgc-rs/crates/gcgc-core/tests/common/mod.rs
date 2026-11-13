//! Shared helpers for golden-output tests. The golden files are produced from the original Python
//! code by `port-notes/tools/make_golden.sh`.
#![allow(dead_code)]

use gcgc_core::{build_run, Column, LogFile, Out, Run, RunOptions, Table};
use serde_json::Value;
use std::path::PathBuf;

pub fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..").canonicalize().unwrap()
}

pub fn golden() -> PathBuf {
    let g = repo().join("port-notes/golden");
    assert!(g.exists(), "golden outputs missing: run port-notes/tools/make_golden.sh");
    g
}

pub fn dataset(name: &str) -> String {
    let bytes = std::fs::read(repo().join("datasets").join(name)).unwrap();
    String::from_utf8_lossy(&bytes).into_owned()
}

pub fn load(path: PathBuf) -> Value {
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"))).unwrap()
}

pub fn log_files() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(repo().join("datasets"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| !n.ends_with(".md") && !n.ends_with(".csv"))
        .collect();
    v.sort();
    v
}

/// Build the runs of a golden notebook exactly as its parse cell did.
pub fn notebook_runs(nb: &str) -> (Vec<Run>, Value) {
    let loaded = load(golden().join(format!("notebooks/{nb}/tables_after_load.json")));
    let zero_times = nb != "universal_plotting";
    let mut runs = Vec::new();
    for (files, label) in loaded["files"].as_array().unwrap().iter().zip(loaded["labels"].as_array().unwrap()) {
        let files: Vec<LogFile> = files
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                let name = p.as_str().unwrap().rsplit('/').next().unwrap().to_string();
                LogFile { text: dataset(&name), name }
            })
            .collect();
        let mut out = Out::new();
        runs.push(build_run(
            label.as_str().unwrap(),
            &files,
            RunOptions { zero_times, ignore_crashes: false },
            &mut out,
        ));
    }
    (runs, loaded)
}

/// Compare a table with a golden column-oriented dump. Returns a list of human-readable mismatches.
pub fn diff_table(ours: &Table, golden: &Value, what: &str) -> Vec<String> {
    let mut errs = Vec::new();
    let cols: Vec<&str> = golden["columns"].as_array().unwrap().iter().map(|c| c.as_str().unwrap()).collect();
    let names: Vec<&str> = ours.names().iter().map(String::as_str).collect();
    if names != cols {
        errs.push(format!("{what}: column order differs\n  ours:   {names:?}\n  golden: {cols:?}"));
    }
    let rows = golden["index"].as_array().unwrap().len();
    if ours.len() != rows {
        errs.push(format!("{what}: {} rows, golden has {rows}", ours.len()));
        return errs;
    }
    for c in cols {
        let g = golden["data"][c].as_array().unwrap();
        let Some(col) = ours.column(c) else {
            errs.push(format!("{what}: missing column {c}"));
            continue;
        };
        let mut bad = 0;
        let mut first = None;
        for (r, gv) in g.iter().enumerate() {
            let ok = match (col, gv) {
                (Column::Num(v), Value::Null) => v[r].is_none(),
                (Column::Num(v), Value::Number(n)) => v[r] == Some(n.as_f64().unwrap()),
                (Column::Str(v), Value::Null) => v[r].is_none(),
                (Column::Str(v), Value::String(s)) => v[r].as_deref() == Some(s.as_str()),
                _ => false,
            };
            if !ok {
                bad += 1;
                first.get_or_insert(r);
            }
        }
        if let Some(r) = first {
            errs.push(format!(
                "{what}: column {c}: {bad} mismatches, first at row {r}: ours={:?} golden={}",
                col.get(r),
                g[r]
            ));
        }
    }
    errs
}
