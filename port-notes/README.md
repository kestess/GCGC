# GCGC → Rust + WebAssembly: port notes

Written 2026-11-12 after reading every Python file, all three notebooks, the tests and the docs.

## 1. What the repo actually is

GCGC is a Jupyter-notebook tool. You point it at JVM GC logs (`-Xlog:gc*`) and it draws about 20 plots and 2 ASCII tables.
The Python logic is only **about 1,150 lines**:

| File | Lines | Role |
|---|---|---|
| `src/parse_log_file.py` | 229 | Builds one giant regex (30 capture groups) plus column names and types |
| `src/read_log_file.py` | 449 | Line-by-line regex → column dict → DataFrame. Derived columns, time scaling, concat runs |
| `src/filter_and_group.py` | 203 | Row filter (a Python predicate) and group-by-column → lists of (x, y) series, labels, colors |
| `src/graphing/plotting.py` | 958 | ~17 plot functions: bucketing, sums, percentiles, histograms |
| `src/graphing/heatmap.py` | 454 | Linear and log latency heatmaps (mostly copied matplotlib-docs code) |
| `src/graphing/{summary,percentiles,allocation_rate,code_cache_flushing_rate}.py` | ~370 | Stats and ASCII tables |
| `src/notebooks/GCGC.ipynb` | 58 cells | **The actual product**: filepaths and labels → 20 numbered analyses |

### How much of each big library is used

- **pandas**: only as a column store. It uses DataFrame-from-dict, `concat`, `replace(nan→None)`, `copy`, row-wise `apply(filter)`, column access, `min`/`max`/`iloc`, `sort_values`, `empty`, and one `read_csv`. No joins, pivots or real groupby (grouping is a hand-written dict loop). → A ~200-line typed column struct in Rust.
- **numpy**: `percentile` (linear interpolation), `mean`, `std` (population, ddof=0), `rot90`. → ~50 lines.
- **matplotlib**: scatter, line, bar, (broken) pie, `imshow` heatmap with a `plasma` colormap and colorbar, legends outside the axes, log y-scale, rotated ticks, and `dates.date2num`. **This is the only part that's real work.** Rendering should be delegated (see §3).
- **Jupyter**: the UX. Users edit `filepaths`/`labels`, write little `filter_by` predicates, and tweak returned axes (`set_title`, `set_yscale('log')`). The web app has to replace this with UI.

### The data flow (this is the seam that makes a piecemeal port possible)

```
log files ─► regex per line ─► column table ─► derived columns ─► [filter] ─► [group_by] ─► (x,y) series ─► stats/bucketing ─► render
             parse_log_file     read_log_file                      filter_and_group                       graphing/*            matplotlib
```

Every stage is a pure function of the previous stage's output. Nothing is stateful except the notebook kernel holding `gc_event_dataframes`.

## 2. Spike result: the regex ports exactly ✅

The biggest technical risk was the parser regex. It uses lookaheads, including **a capture group inside a lookahead** (`(?= ?(\d+\.\d+)ms)`). Rust's standard `regex` crate does not support lookaround at all.

I tested it with `fancy-regex` (a backtracking engine with Perl/Python semantics, pure Rust, so it compiles to wasm):

- `golden.py` imports `src/parse_log_file.py` as-is (stdlib `re` only) and dumps every match's 30 groups as JSON for all 18 log files in `datasets/`.
- `spike/` compiles the **exact same regex string** with fancy-regex and diffs.
- **Result: 41,958 / 41,958 matched lines identical in all 30 groups, across all 18 files.** About 0.7 s for ~15 MB in release mode, including JSON loading.

The one subtlety: Python iterates lines *with* the trailing `\n`, and Python's `$` matches before a final newline. Rust's `$` doesn't. Strip `\n` (and `\r`) before matching; `.` never crosses newlines in either engine, so stripping is equivalent.

To re-run: `cd port-notes && tools/make_golden.sh && cd spike && cargo run --release`.

Coverage gaps found while profiling the captures (the test data doesn't exercise these):
- **Safepoint columns are never populated** by any dataset, so the safepoint/`TotalApplicationThreadPauseTime` branches are untested. Get a log with `-Xlog:safepoint` before trusting them.
- **DateTime-stamped lines: only 2 rows** in the whole corpus. Every other line uses uptime `[1.234s]`.

## 3. Recommended architecture

```
gcgc/                       (cargo workspace)
├─ crates/gcgc-core/        pure Rust, no I/O, no wasm deps
│   ├─ parse.rs             regex + group→column mapping + coalescing + type conversion
│   ├─ table.rs             EventTable { columns: IndexMap<String, Column> }, Column = F64(Vec<Option<f64>>) | Str(Vec<Option<String>>) | I64(..)
│   ├─ derive.rs            scale_time, heap %, metaspace→MB, code-heap %, zero_times, crash fix
│   ├─ query.rs             Filter (declarative), group_by → Vec<Series{label, x, y, color}>
│   ├─ stats.rs             percentile (numpy linear), mean, std, rot90
│   ├─ analyses/*.rs        one fn per notebook analysis → ChartSpec | TextTable
│   └─ format.rs            Python-compatible float formatting for the ASCII tables
├─ crates/gcgc-wasm/        wasm-bindgen façade: parse_run(files) → handle; analysis(id, opts) → JSON ChartSpec
├─ crates/gcgc-py/          (optional) PyO3 bindings so the existing notebook can call Rust
├─ web/                     TS + Vite; file drop, run/label editor, chart rendering, Web Worker
└─ tools/golden/            Python scripts that dump reference output from the original code
```

Key decisions:

1. **Rust produces data, not pixels.** Every analysis returns a renderer-agnostic `ChartSpec` (kind, series, axis labels, log flags, legend, heatmap matrix and bucket labels) as JSON. That makes parity testable (compare numbers, not images) and keeps the wasm small.
2. **Render in JS.** Use **Plotly.js** (it has scatter, line, bar, heatmap with colorbar, `plasma`-like colorscales, log axes and legends; closest to matplotlib with the least code) or **ECharts**. uPlot is faster but needs a separate heatmap. The pure-Rust alternative is `plotters` + `plotters-canvas`/SVG. It works in wasm, but you'd rebuild legends, colorbars and tick logic by hand, and charts wouldn't be interactive. Only worth it if you need identical PNG output in a CLI too.
3. **Filters become data, not code.** All ~12 filter functions in the notebook are one of: `col == "Pause"`, `col in {"Pause","Concurrent"}`, `col is not None`, `col` *truthy* (≠ None and ≠ 0), or an AND of those. Model them as `enum Filter { Eq, In, NotNull, Truthy, And(Vec<Filter>) }`. Escape hatch for power users: an optional JS callback per row, or a tiny expression language later.
4. **Files instead of globs.** `ZGC.log.*` (several files → one run) becomes "drop several files into one run slot". **Sort them by name.** Python's `glob` order is filesystem-dependent, which makes concat order (and so line plots) non-deterministic in the original.
5. **Parsing in a Web Worker.** Real GC logs can be hundreds of MB. Stream with `File.stream()` → chunk → split lines in Rust. The spike already shows the regex speed is fine.
6. **`print` warnings become diagnostics.** The Python code reports problems via `print` ("Not enough labels", heatmap out-of-range counts, unknown units). Return `Vec<Diagnostic>` alongside each result and show them in the UI.

## 4. Yes, it can be done piecemeal. Recommended order

The trick is **one Rust core with two frontends**. PyO3 lets you swap Rust into the *existing notebook* one function at a time and check it against the Python. The same crate compiles to wasm for the browser.

| Phase | Deliverable | How you know it's right |
|---|---|---|
| **0. Oracle** ✅ done | `tools/make_golden.sh` → `golden/`: regex captures, parsed tables, full tables per run, and every notebook cell's plot-call data, ASCII text and PNG for all 3 notebooks. See [golden.md](golden.md) | n/a (this *is* the reference) |
| **1. Parser** ✅ spiked | `gcgc-core::parse`: regex, group→column coalescing (e.g. `HeapBeforeGC` comes from group 9 *or* 13), type casts, safepoint `EventType` fixup | Byte-exact vs. golden (a), pre-derive |
| **2. Derived columns** | scale_time, HeapPercentFull rules (GenZGC minor filter, Young/Incremental tags, `Mixed`), metaspace K/M/G→MB, 6 code-heap % columns, multi-file concat, zero_times | Golden (a), float tolerance 1e-9 |
| **2b. (optional) PyO3** | `gcgc_py.get_gc_event_tables()` returns a pandas DataFrame (via `pyo3-polars`/Arrow, or just dict-of-lists). Swap it into the notebook. The notebook runs unchanged on the Rust parser. | Existing `test/*.py` unit tests pass against it |
| **3. Query + stats** | Filter enum, group_by (sorted keys, the `"( col = None )"` bucket), percentile/mean/std | Unit tests vs numpy on random vectors |
| **4. Analyses** | 20 fns → `ChartSpec`/`TextTable`. Port in notebook order; each is 20–80 lines | Golden (b) and (c) |
| **5. wasm + web MVP** | `wasm-pack build`, Vite app: run editor (files + label), render analyses #1, #2, #16, #17 first | Visual check against the notebook PNGs |
| **6. Rest of UI** | Heatmap dimension inputs, interval inputs, percentile lists, log toggles, CSV overlay (#21), export PNG/SVG/JSON | |

Each phase ships something usable. After phase 2b the notebook users already benefit, and after phase 5 there's a working web page.

### Even more piecemeal: Pyodide as phase −1

Pyodide ships pandas, numpy and matplotlib compiled to wasm. **The existing code can run in a browser today, unmodified** (or via JupyterLite, which runs the notebook itself). Cost: ~25–40 MB first load and slower parsing. You could ship that as the v0 web app, then replace modules with Rust-wasm behind the same Python function names. It's a strangler pattern entirely inside the browser. Downside: two runtimes in one page during the transition. I'd use Pyodide mainly as a *parity oracle you can run in CI without a Python install*, not as the shipping path.

### Honest alternative: plain TypeScript

JS `RegExp` supports lookahead with captures too, with backtracking semantics like Python's. So the whole core could be ~1,500 lines of TS with no wasm. Rust/wasm earns its keep if you want (a) one core reused by a CLI, the PyO3 notebook bridge *and* the web app, (b) multi-hundred-MB logs, or (c) stronger typing over the column table. For this workload alone, TS would also be fine.

## 5. Parity traps (behaviors you must replicate or deliberately fix)

**Semantics to replicate**
- **Truthiness ≠ not-null.** `duration_present` and `diff_in_entries_filter` use `if row[x]:`, so **0.0 values are dropped**. The heatmap's `if pause:` also skips 0 ms pauses. `NotNull` and `Truthy` must be separate filter ops.
- `np.percentile` = linear interpolation between closest ranks; `np.std` = population std.
- `group_by`: the key is the cell value. `None` *and empty string* go to the bucket `"( <col> = None )"`. Keys are sorted as strings (Python code-point order = Rust `String` `Ord`, so this matches). Labels are `"<runlabel>: <key>"`.
- Multi-value column coalescing: the first non-None group among aliases wins, in the regex's group order.
- `scale_time` reads the unit from **row 0 only** and applies it to every row.
- `HeapPercentFull`: if *any* row has a prepopulated value (ZGC/GenZGC), use those, but keep only rows whose EventType contains "Collection" and not "Minor". Otherwise compute `100*after/max` only for non-young (no "Young"/"Incremental" in EventType/EventName) **or** `AdditionalEventInfo == "Mixed"`. That exact equality **never matches** on the sample data: captured values look like `"(Mixed) "` with parens and a trailing space, so G1 mixed collections are excluded in practice. Decide whether to keep that or fix it.
- `get_buckets_and_range` seeds min from `groups[0][0]` (crashes on an empty first group), and `number_of_buckets = int((max-min)/interval) + 1`.
- Allocation rate: `(before[i+1] - after[i]) / Δt`, aggregated when elapsed ≥ interval. Note `interval_duration=None` raises a TypeError in Python.
- Code-cache flush rate: counts flushes since the last strictly-greater timestamp, rate = n/Δt, starting at t = 0.
- ASCII tables: `"%9.4f"` right-padded to 13/18 columns, labels truncated to the *last* 15 chars, `A/B/C…` legend substitution when labels are too long. Python `round()` is banker's rounding on the binary value. Use a careful `round_half_even` or just format with `{:.4}` and accept rare last-digit diffs.

**Bugs in the original (port the *intent*, record the divergence)**
- `ignore_crashes=True` is broken: `fix_timing_errors` reads `TimeFromStart_seconds` *before* `scale_time` creates it → KeyError. It also uses chained assignment, which is a silent no-op under pandas Copy-on-Write. Implement it after time scaling.
- `plot_pie_sum` is broken: it unpacks 4 of 5 return values and appends the builtin `slice` instead of `pslice`. Not used by the notebook, so drop it or fix it.
- `generate_new_column_with_values_in_mb` discards the result of `drop`, so `UsedMetaspaceAfterGCWithUnit` remains. Harmless.
- `populate_buckets(include_timing_in_bucket=True)` iterates `range(len(...))` instead of the timestamps.
- The `DateTime` timing path (`date2num` on strings, `__remove_datetime_scaling` returning a Series of lists) is effectively unusable. In Rust, parse with `chrono` `%Y-%m-%dT%H:%M:%S%.3f%z` and convert to seconds.
- `get_heatmap_data_logarithmic` computes `get_y_bucket_number` then overwrites it with the binary search. Only the binary search matters. Its first-bucket semantics: values below `ranges[0]` → −1 (dropped as out-of-range).
- `plotting.py` imports `from filter_and_group import ...` (not `src.`). It only works because the notebook adds `../` to `sys.path`.

**Things the web app loses unless you add UI for them**
- Arbitrary Python filters and `grouping_function` callables (the universal-plotting tutorial).
- Post-hoc axes tweaking (title, log scale, `ScalarFormatter`). Expose title, axis labels, log-y and line/scatter as per-chart controls.
- Overlaying an external CSV on a plot (analysis #21). Parse CSV in Rust (`csv` crate) or JS and allow `column`/`column_timing` selection.
- In-place `sort_values` in cell 22 mutates the shared dataframes for all later cells. In Rust, just sort each run by time once after concat (equivalent for every later plot).

## 6. Rough size and effort

- Rust core: ~2–3k lines. wasm façade ~200. Web UI ~1.5–2.5k TS.
- wasm binary: fancy-regex + regex-automata + serde ≈ 0.6–1.2 MB uncompressed (~300 KB gzipped) vs ~30 MB for Pyodide + pandas + matplotlib. *Estimate, not measured.* I haven't built for `wasm32-unknown-unknown` yet (target not installed). Every dependency named is pure Rust and known to build for wasm.
- Effort: phases 0–2 a few days. Phase 4 (20 analyses with golden tests) about a week. Web UI quality is the open-ended part.

## 7. Next concrete steps

1. ~~Venv + original tests~~ ✅ and ~~golden outputs~~ ✅. See [golden.md](golden.md), including the unstable-sort caveat for comparing plots after cell 22.
2. Promote `spike/` into `crates/gcgc-core` with `parse.rs` and `table.rs`. Integration tests diff against `golden/regex` and `golden/parsed`.
3. Phase 2 derived columns, tested against `golden/notebooks/*/tables_after_load.json`.
4. Decide: Plotly.js vs ECharts, and whether the PyO3 bridge (phase 2b) is wanted.

## 8. Status — 2026-11-12 (implemented)

The port exists in [`../gcgc-rs`](../gcgc-rs/README.md). Phases 0–6 are done, and 2b (PyO3) was skipped.

- **Engine (`gcgc-core`)**: parser, run building, filters/grouping, every plotting function used by the
  notebooks, heatmaps, ASCII tables, and 21 notebook analyses plus the tutorial and a custom chart.
- **Parity**: exact against the golden outputs. That covers 41,958 regex captures, 18 parsed tables, 13 run
  tables (including pandas' unstable sort order, reproduced by porting numpy's introsort), and all 47
  plotting cells' call data and stdout, bit for bit. A deliberate-mutation check confirmed the suite
  catches small changes (a 0.01 alpha change, a 1e-12 shift in a rate).
- **wasm**: output identical to native for every analysis. 1.9 MB raw / ≈600 KB gzipped.
- **Web app**: drop logs, rotated-file grouping, run rename/reorder/remove/CSV export, CSV overlay,
  per-analysis settings, lazy rendering in a Web Worker, light/dark themes, a static `dist/` build.
- **Performance**: a 116 MB log parses in ≈3.3 s in the browser.

Things that turned out differently from the plan above:
- serde_json needs the `float_roundtrip` feature for exact golden comparisons. Its default parser can
  be one ulp off, which first looked like an engine bug.
- numpy-vs-Python rounding matters: `round()` on a grouped (float64 Series) value uses numpy's
  scale-rint-unscale, while ungrouped values are Python floats with correct rounding. Both are reproduced.
- Plotly.js has no "Plasma" colorscale; the app embeds matplotlib's plasma stops.

Open ideas: PyO3 bindings for the notebook; streaming parse for multi-GB logs (currently the whole
file is held in memory); safepoint-log test data (the corpus has none); an event-table viewer.
