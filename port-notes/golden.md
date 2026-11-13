# Golden outputs (reference data from the original Python)

Regenerate (≈16 s): `tools/make_golden.sh`. It is deterministic: two runs give byte-identical JSON.
`golden/` (~39 MB) is git-ignored because it's fully reproducible.

## Environment
`.venv`: Python 3.9.23 (the repo says it was tested on 3.9.5), numpy 1.24.4, pandas 1.5.3, matplotlib 3.7.5
(`requirements-golden.txt`). The pins are deliberate: matplotlib ≥3.9 removed `cm.get_cmap` (used by heatmap.py),
and pandas ≥2/3 changes Copy-on-Write and dtype behavior. The original `test/*.py` suites pass in this env (12 tests).

```
uv venv -p /opt/homebrew/bin/python3.9 .venv
uv pip install -p .venv/bin/python -r requirements-golden.txt
```

## Layout
| Path | Producer | Contents |
|---|---|---|
| `golden/regex/regex.txt`, `columns.json` | `tools/regex_captures.py` | The generated regex, its 30 column names and types |
| `golden/regex/captures/<log>.json` | same | `[line_no, g1..g30]` for every matching line (stdlib `re` only) |
| `golden/parsed/<log>.json` | `tools/dump_parsed.py` | `get_parsed_data_from_file()` per file: typed, coalesced, safepoint fixup, **before** derived columns |
| `golden/notebooks/<nb>/tables_after_load.json` | `tools/run_notebook.py` | `files`, `labels`, and every `gc_event_dataframes` table right after the parse cell (all derived columns, zero_times) |
| `golden/notebooks/<nb>/tables_after_sort.json` | same | Same tables after cell 22's in-place `sort_values` |
| `golden/notebooks/<nb>/cell_NN.json` | same | `source`, `stdout` (ASCII tables), `error`, and per figure/axes: every `plot/scatter/bar/pie/imshow/text` call with args + key kwargs (label, color, alpha, marker, width…), plus final title, labels, scales, limits, ticks, legend |
| `golden/notebooks/<nb>/cell_NN_figK.png` | same | Rendered figure (60 dpi), for eyeballing the web version |
| `golden/notebooks/<nb>/summary.json` | same | Per-cell figure/call counts and status |

Notebooks: `GCGC` (7 runs, 25 code cells), `FeatureExamples` (6 runs incl. metaspace and code cache, 25 cells),
`universal_plotting` (2 cells). **All cells run without exceptions.**

JSON conventions: tables are column-oriented `{columns, dtypes, index, data}`. NaN → `null`, ±inf → `"inf"`/`"-inf"`.
Matplotlib Text objects → their string.

## Deviations from running the notebook by hand
- `glob` results are **sorted** (the original order depends on the filesystem). Affects only `ZGC.log.*` → `[ZGC.log.0, ZGC.log.1]`.
- `%` magics and `!pip install` lines are stripped. Backend is Agg.
- Cells run in one shared namespace, in order, like "Run All".

## Findings while generating
- **The notebook's sort is unstable.** Cell 22 calls `sort_values` (default quicksort), which reorders rows with
  equal timestamps (e.g. 233 tie inversions in GenZGC, 5,220 in the code-cache run). Every later cell plots from the
  re-sorted tables. Rust can't reproduce numpy's introsort tie order, so **compare series after cell 22 as
  multisets within equal-x runs**, or compare line plots by (x, sorted ys). Before cell 22, order is file order and exact.
- **Duplicate index labels after concat**: the ZGC run (2 files) has 332 duplicated pandas index labels because
  each file's frame starts at 0. Harmless for the plots, but don't key anything by pandas index.
- The **summary table prints the event count as a float** (`122.0000`) because it goes through `"%9.4f"`.
- GCGC cells 30–36 (code heap and code-cache flushing) produce empty figures for that file set (no codeheap lines).
  FeatureExamples covers them.
