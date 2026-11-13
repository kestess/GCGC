//! Parsing and run-building parity with the original Python implementation.

mod common;

use common::*;
use gcgc_core::parse::{captures, parse_log, parsing_groups};
use gcgc_core::run::sort_by_time;

#[test]
fn regex_string_is_identical() {
    let (rx, cols, types) = parsing_groups();
    let golden_rx = std::fs::read_to_string(golden().join("regex/regex.txt")).unwrap();
    assert_eq!(rx, golden_rx);
    let meta = load(golden().join("regex/columns.json"));
    let gcols: Vec<&str> = meta["columns"].as_array().unwrap().iter().map(|c| c.as_str().unwrap()).collect();
    assert_eq!(cols, gcols);
    let gtypes: Vec<String> =
        meta["types"].as_array().unwrap().iter().map(|t| t.as_str().unwrap().to_string()).collect();
    let ours: Vec<String> = types
        .iter()
        .map(|t| {
            match t {
                gcgc_core::parse::DType::Str => "str",
                gcgc_core::parse::DType::Float => "float",
                gcgc_core::parse::DType::Int => "int",
            }
            .to_string()
        })
        .collect();
    assert_eq!(ours, gtypes);
}

#[test]
fn raw_captures_match() {
    for name in log_files() {
        let golden = load(golden().join(format!("regex/captures/{name}.json")));
        let golden = golden.as_array().unwrap();
        let ours = captures(&dataset(&name));
        assert_eq!(ours.len(), golden.len(), "{name}: matched line count");
        for (o, g) in ours.iter().zip(golden) {
            let g = g.as_array().unwrap();
            assert_eq!(o.0 as u64, g[0].as_u64().unwrap(), "{name}: line number");
            for (i, v) in o.1.iter().enumerate() {
                assert_eq!(v.as_deref(), g[i + 1].as_str(), "{name}: line {} group {}", o.0, i + 1);
            }
        }
    }
}

#[test]
fn parsed_tables_match() {
    let mut errs = Vec::new();
    for name in log_files() {
        let (table, diags) = parse_log(&dataset(&name));
        assert!(diags.is_empty(), "{name}: {diags:?}");
        errs.extend(diff_table(&table, &load(golden().join(format!("parsed/{name}.json"))), &name));
    }
    assert!(errs.is_empty(), "\n{}", errs.join("\n"));
}

#[test]
fn run_tables_match_after_load() {
    let mut errs = Vec::new();
    for nb in ["GCGC", "FeatureExamples", "universal_plotting"] {
        let (runs, loaded) = notebook_runs(nb);
        let tables = loaded["tables"].as_array().unwrap();
        assert_eq!(runs.len(), tables.len(), "{nb}: run count");
        for (run, g) in runs.iter().zip(tables) {
            errs.extend(diff_table(&run.table, g, &format!("{nb}/{}", run.label)));
        }
    }
    assert!(errs.is_empty(), "\n{}", errs.join("\n"));
}

#[test]
fn run_tables_match_after_notebook_sort() {
    let mut errs = Vec::new();
    for nb in ["GCGC", "FeatureExamples"] {
        let (runs, _) = notebook_runs(nb);
        let sorted = load(golden().join(format!("notebooks/{nb}/tables_after_sort.json")));
        for (run, g) in runs.iter().zip(sorted["tables"].as_array().unwrap()) {
            errs.extend(diff_table(&sort_by_time(&run.table), g, &format!("{nb}/{} (sorted)", run.label)));
        }
    }
    assert!(errs.is_empty(), "\n{}", errs.join("\n"));
}
