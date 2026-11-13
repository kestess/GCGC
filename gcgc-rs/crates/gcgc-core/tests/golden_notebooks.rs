//! Every notebook cell's plots and printed output, compared with the original Python run.

mod common;

use common::*;
use gcgc_core::analyses::{self, Ctx, External, Params};
use gcgc_core::chart::{Call, Color, Scale};
use gcgc_core::csv::read_csv;
use gcgc_core::run::sort_by_time;
use gcgc_core::Table;
use serde_json::Value;

/// (notebook cell number, analysis id)
const CELLS: &[(u32, &str)] = &[
    (7, "stw_pauses"),
    (9, "stw_pauses_by_name"),
    (11, "latency_heatmap"),
    (13, "latency_heatmap_log"),
    (15, "latency_percentiles"),
    (17, "pause_sum_intervals"),
    (20, "heap_percent_full"),
    (22, "heap_before_after"),
    (24, "reclaimed"),
    (26, "allocation_rate"),
    (30, "code_heap_non_profiled"),
    (32, "code_heap_profiled"),
    (34, "code_heap_non_nmethods"),
    (36, "code_cache_flushing"),
    (39, "metaspace"),
    (42, "concurrent_durations"),
    (44, "event_duration_sums"),
    (46, "pause_summary"),
    (48, "pause_percentiles"),
    (50, "event_mean_sum"),
    (52, "pause_histogram"),
    (54, "gc_frequency"),
    (56, "external_overlay"),
];

/// Titles deliberately changed from the notebook (typos / misleading copy-paste).
const TITLE_CHANGES: &[&str] = &["pause_histogram", "external_overlay", "heap_after_gc_percent"];

fn num_eq(ours: f64, golden: &Value) -> bool {
    match golden {
        Value::Number(n) => n.as_f64() == Some(ours),
        _ => false,
    }
}

fn opt_eq(ours: Option<f64>, golden: &Value) -> bool {
    match (ours, golden) {
        (None, Value::Null) => true,
        (Some(x), g) => num_eq(x, g),
        _ => false,
    }
}

fn series_diff(what: &str, ours: &[Option<f64>], golden: &Value, errs: &mut Vec<String>) {
    let g = golden.as_array().unwrap();
    if ours.len() != g.len() {
        errs.push(format!("{what}: length {} vs golden {}", ours.len(), g.len()));
        return;
    }
    let bad: Vec<usize> = (0..g.len()).filter(|&i| !opt_eq(ours[i], &g[i])).collect();
    if let Some(&i) = bad.first() {
        errs.push(format!("{what}: {} mismatches, first [{i}] ours={:?} golden={}", bad.len(), ours[i], g[i]));
    }
}

fn color_eq(ours: &Option<Color>, golden: Option<&Value>) -> bool {
    match (ours, golden) {
        (None, None) => true,
        (Some(Color::Named(n)), Some(Value::String(s))) => n == s,
        (Some(Color::Rgb(c)), Some(Value::Array(a))) => {
            a.len() == 3 && c.iter().zip(a).all(|(x, y)| y.as_f64() == Some(*x))
        }
        _ => false,
    }
}

fn kw_num(ours: Option<f64>, golden: Option<&Value>) -> bool {
    match (ours, golden) {
        (None, None) => true,
        (Some(x), Some(g)) => num_eq(x, g),
        _ => false,
    }
}

fn kw_str(ours: &Option<String>, golden: Option<&Value>) -> bool {
    ours.as_deref() == golden.and_then(Value::as_str)
}

fn compare_call(what: &str, ours: &Call, g: &Value, errs: &mut Vec<String>) {
    let method = g["method"].as_str().unwrap();
    if ours.method() != method {
        errs.push(format!("{what}: method {} vs golden {method}", ours.method()));
        return;
    }
    let args = g["args"].as_array().unwrap();
    let kw = &g["kwargs"];
    let mut kwbad = Vec::new();
    match ours {
        Call::Plot { x, y, label, color, alpha, marker, linestyle, markersize } => {
            series_diff(&format!("{what} x"), x, &args[0], errs);
            series_diff(&format!("{what} y"), y, &args[1], errs);
            if !kw_str(label, kw.get("label")) {
                kwbad.push("label");
            }
            if !color_eq(color, kw.get("color")) {
                kwbad.push("color");
            }
            if !kw_num(*alpha, kw.get("alpha")) {
                kwbad.push("alpha");
            }
            if !kw_str(marker, kw.get("marker")) {
                kwbad.push("marker");
            }
            if !kw_str(linestyle, kw.get("linestyle")) {
                kwbad.push("linestyle");
            }
            if !kw_num(*markersize, kw.get("markersize")) {
                kwbad.push("markersize");
            }
        }
        Call::Scatter { x, y, label, color, alpha } => {
            series_diff(&format!("{what} x"), x, &args[0], errs);
            series_diff(&format!("{what} y"), y, &args[1], errs);
            if !kw_str(label, kw.get("label")) {
                kwbad.push("label");
            }
            if !color_eq(color, kw.get("color")) {
                kwbad.push("color");
            }
            if !kw_num(*alpha, kw.get("alpha")) {
                kwbad.push("alpha");
            }
        }
        Call::Bar { x, height, width, label, color, alpha } => {
            if !num_eq(*x, &args[0]) || !num_eq(*height, &args[1]) {
                errs.push(format!("{what}: bar ({x}, {height}) vs golden ({}, {})", args[0], args[1]));
            }
            if !kw_num(*width, kw.get("width")) {
                kwbad.push("width");
            }
            if !kw_str(label, kw.get("label")) {
                kwbad.push("label");
            }
            if !color_eq(color, kw.get("color")) {
                kwbad.push("color");
            }
            if !kw_num(*alpha, kw.get("alpha")) {
                kwbad.push("alpha");
            }
        }
        Call::Imshow { data, vmin, cmap } => {
            let g = args[0].as_array().unwrap();
            let gd: Vec<Vec<f64>> =
                g.iter().map(|r| r.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect()).collect();
            if *data != gd {
                errs.push(format!("{what}: imshow data differs\n   ours  {data:?}\n   golden {gd:?}"));
            }
            if !kw_num(Some(*vmin), kw.get("vmin")) {
                kwbad.push("vmin");
            }
            if kw.get("cmap").and_then(Value::as_str) != Some(cmap.as_str()) {
                kwbad.push("cmap");
            }
        }
        Call::Text { x, y, text, color } => {
            if !num_eq(*x, &args[0]) || !num_eq(*y, &args[1]) || args[2].as_str() != Some(text.as_str()) {
                errs.push(format!("{what}: text ({x},{y},{text}) vs golden {:?}", args));
            }
            if kw.get("color").and_then(Value::as_str) != Some(color.as_str()) {
                kwbad.push("color");
            }
        }
    }
    if !kwbad.is_empty() {
        errs.push(format!("{what}: kwargs differ {kwbad:?}: ours {ours:?}\n   golden kwargs {kw}"));
    }
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

fn check_cell(nb: &str, cell: u32, id: &str, ctx: &Ctx, errs: &mut Vec<String>) {
    let golden = load(golden().join(format!("notebooks/{nb}/cell_{cell:02}.json")));
    let what = format!("{nb} cell {cell} ({id})");
    let ours = match analyses::run(id, ctx, &Params::default()) {
        Ok(o) => o,
        Err(e) => {
            errs.push(format!("{what}: error {e}"));
            return;
        }
    };
    let gstdout = golden["stdout"].as_str().unwrap();
    if ours.stdout != gstdout {
        errs.push(format!("{what}: stdout differs\n--- ours ---\n{}--- golden ---\n{gstdout}", ours.stdout));
    }
    let gfigs = golden["figures"].as_array().unwrap();
    if ours.figures.len() != gfigs.len() {
        errs.push(format!("{what}: {} figures vs golden {}", ours.figures.len(), gfigs.len()));
        return;
    }
    for (fi, (fig, gfig)) in ours.figures.iter().zip(gfigs).enumerate() {
        let what = format!("{what} fig{fi}");
        let gax = &gfig[0];
        let ax = &fig.axes;
        let gcalls = gax["calls"].as_array().unwrap();
        if ax.calls.len() != gcalls.len() {
            errs.push(format!("{what}: {} calls vs golden {}", ax.calls.len(), gcalls.len()));
        }
        for (ci, (c, g)) in ax.calls.iter().zip(gcalls).enumerate() {
            compare_call(&format!("{what} call{ci}"), c, g, errs);
        }
        let glegend = &gax["legend"];
        match (ax.legend, glegend) {
            (false, Value::Null) => {}
            (true, Value::Array(_)) => {
                if fig.legend_labels != strs(glegend) {
                    errs.push(format!("{what}: legend {:?} vs golden {}", fig.legend_labels, glegend));
                }
            }
            _ => errs.push(format!("{what}: legend called={} vs golden {}", ax.legend, glegend)),
        }
        if let Some(t) = &ax.xticks {
            let g: Vec<f64> = gax["xticks"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
            if *t != g {
                errs.push(format!("{what}: xticks {t:?} vs golden {g:?}"));
            }
        }
        if let Some(l) = &ax.xticklabels {
            if *l != strs(&gax["xticklabels"]) {
                errs.push(format!("{what}: xticklabels {l:?} vs golden {}", gax["xticklabels"]));
            }
        }
        if let Some(l) = &ax.yticklabels {
            if *l != strs(&gax["yticklabels"]) {
                errs.push(format!("{what}: yticklabels {l:?} vs golden {}", gax["yticklabels"]));
            }
        }
        let gscale = gax["yscale"].as_str().unwrap();
        let scale = if ax.yscale == Scale::Log { "log" } else { "linear" };
        if scale != gscale {
            errs.push(format!("{what}: yscale {scale} vs golden {gscale}"));
        }
        if !TITLE_CHANGES.contains(&id) && ax.title != gax["title"].as_str().unwrap() {
            errs.push(format!("{what}: title {:?} vs golden {}", ax.title, gax["title"]));
        }
    }
}

fn external() -> External {
    let text = std::fs::read_to_string(repo().join("datasets/example_external_data.csv")).unwrap();
    External {
        label: "Reads completed by application".into(),
        table: read_csv(&text).unwrap(),
        x_column: "Time Seconds".into(),
        y_column: "Reads Completed".into(),
    }
}

fn check_notebook(nb: &str) -> Vec<String> {
    let (runs, _) = notebook_runs(nb);
    let sorted: Vec<Table> = runs.iter().map(|r| sort_by_time(&r.table)).collect();
    let ext = external();
    let ctx = Ctx {
        labels: runs.iter().map(|r| r.label.clone()).collect(),
        loaded: runs.iter().map(|r| &r.table).collect(),
        sorted: sorted.iter().collect(),
        external: Some(&ext),
    };
    let mut errs = Vec::new();
    for (cell, id) in CELLS {
        check_cell(nb, *cell, id, &ctx, &mut errs);
    }
    errs
}

#[test]
fn gcgc_notebook_matches() {
    let errs = check_notebook("GCGC");
    assert!(errs.is_empty(), "{} problems:\n{}", errs.len(), errs.join("\n"));
}

#[test]
fn feature_examples_notebook_matches() {
    let errs = check_notebook("FeatureExamples");
    assert!(errs.is_empty(), "{} problems:\n{}", errs.len(), errs.join("\n"));
}

#[test]
fn universal_plotting_tutorial_matches() {
    let (runs, _) = notebook_runs("universal_plotting");
    let ctx = Ctx {
        labels: runs.iter().map(|r| r.label.clone()).collect(),
        loaded: runs.iter().map(|r| &r.table).collect(),
        sorted: runs.iter().map(|r| &r.table).collect(),
        external: None,
    };
    let mut errs = Vec::new();
    check_cell("universal_plotting", 1, "heap_after_gc_percent", &ctx, &mut errs);
    assert!(errs.is_empty(), "{} problems:\n{}", errs.len(), errs.join("\n"));
}
