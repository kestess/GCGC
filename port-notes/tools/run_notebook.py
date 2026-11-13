"""Layer 2: execute a GCGC notebook headlessly and record what every cell plots and prints.

Usage: run_notebook.py <path/to/notebook.ipynb> <output-name>

For each code cell N, writes golden/notebooks/<name>/cell_NN.json containing:
  - source:  the executed cell code
  - stdout:  everything printed (ASCII tables, warnings)
  - error:   traceback text if the cell raised (execution continues with the next cell)
  - axes:    per axes, in creation order: the data-bearing calls (plot/scatter/bar/pie/imshow/text)
             with their arguments, plus final title/labels/scales/ticks/legend.
and cell_NN_figK.png for each figure.

Also writes tables_after_load.json (gc_event_dataframes right after the parsing cell)
and tables_after_sort.json (after the in-place sort_values in the heap before/after cell).

Determinism: glob results are sorted (the original's glob order is filesystem-dependent).
IPython magics (%...) and shell escapes (!...) are stripped.
"""
import contextlib
import functools
import glob as globmod
import io
import json
import os
import sys
import traceback

import matplotlib

matplotlib.use("Agg")
import matplotlib.axes  # noqa: E402
import matplotlib.pyplot as plt  # noqa: E402

from common import GOLDEN, REPO, dataframe_to_json, to_jsonable, write_json  # noqa: E402

nb_path = os.path.abspath(sys.argv[1])
out_name = sys.argv[2]
out_dir = os.path.join(GOLDEN, "notebooks", out_name)
os.makedirs(out_dir, exist_ok=True)

# --- Deterministic glob -------------------------------------------------------------------------
_orig_glob = globmod.glob
globmod.glob = lambda p, *a, **k: sorted(_orig_glob(p, *a, **k))

# --- Record data-bearing Axes calls -------------------------------------------------------------
RECORDED = ["plot", "scatter", "bar", "pie", "imshow", "text"]
KW_KEEP = ["label", "color", "colors", "alpha", "marker", "linestyle", "markersize", "width",
           "labels", "startangle", "vmin", "cmap", "horizontalalignment", "verticalalignment"]
calls = {}  # id(ax) -> list of call records
_depth = [0]


def _wrap(name, fn):
    @functools.wraps(fn)
    def wrapper(self, *args, **kwargs):
        outer = _depth[0] == 0
        _depth[0] += 1
        try:
            if outer:
                rec = {"method": name, "args": to_jsonable(list(args))}
                rec["kwargs"] = {k: to_jsonable(kwargs[k]) for k in KW_KEEP if k in kwargs}
                calls.setdefault(id(self), []).append(rec)
            return fn(self, *args, **kwargs)
        finally:
            _depth[0] -= 1
    return wrapper


for _m in RECORDED:
    setattr(matplotlib.axes.Axes, _m, _wrap(_m, getattr(matplotlib.axes.Axes, _m)))


def describe_axes(ax):
    legend = ax.get_legend()
    return {
        "title": ax.get_title(),
        "xlabel": ax.get_xlabel(),
        "ylabel": ax.get_ylabel(),
        "xscale": ax.get_xscale(),
        "yscale": ax.get_yscale(),
        "xlim": to_jsonable(ax.get_xlim()),
        "ylim": to_jsonable(ax.get_ylim()),
        "xticks": to_jsonable(ax.get_xticks()),
        "xticklabels": [t.get_text() for t in ax.get_xticklabels()],
        "yticklabels": [t.get_text() for t in ax.get_yticklabels()],
        "legend": [t.get_text() for t in legend.get_texts()] if legend else None,
        "calls": calls.get(id(ax), []),
    }


# --- Execute cells ------------------------------------------------------------------------------
with open(nb_path) as fh:
    nb = json.load(fh)

os.chdir(os.path.dirname(nb_path))  # notebooks use paths relative to their own directory
ns = {"__name__": "__main__"}
summary = []
code_idx = 0
for cell_no, cell in enumerate(nb["cells"]):
    if cell["cell_type"] != "code":
        continue
    src = "".join(cell["source"])
    code = "\n".join(l for l in src.splitlines() if not l.lstrip().startswith(("%", "!")))
    if not code.strip():
        continue
    calls.clear()
    buf = io.StringIO()
    error = None
    with contextlib.redirect_stdout(buf):
        try:
            exec(compile(code, f"{out_name}:cell{cell_no}", "exec"), ns)
        except Exception:
            error = traceback.format_exc()
    figs = [plt.figure(n) for n in plt.get_fignums()]
    record = {
        "cell": cell_no,
        "source": src,
        "stdout": buf.getvalue(),
        "error": error,
        "figures": [[describe_axes(ax) for ax in fig.axes] for fig in figs],
    }
    for k, fig in enumerate(figs):
        fig.savefig(os.path.join(out_dir, f"cell_{cell_no:02d}_fig{k}.png"), dpi=100, bbox_inches="tight")  # like Jupyter inline
    plt.close("all")
    write_json(os.path.join(out_dir, f"cell_{cell_no:02d}.json"), record)

    if "gc_event_dataframes" in ns and not os.path.exists(os.path.join(out_dir, "tables_after_load.json")):
        write_json(os.path.join(out_dir, "tables_after_load.json"),
                   {"files": to_jsonable(ns.get("files")), "labels": to_jsonable(ns.get("labels")),
                    "tables": [dataframe_to_json(df) for df in ns["gc_event_dataframes"]]})
    if "sort_values" in code and "gc_event_dataframes" in ns:
        write_json(os.path.join(out_dir, "tables_after_sort.json"),
                   {"tables": [dataframe_to_json(df) for df in ns["gc_event_dataframes"]]})

    n_calls = sum(len(a["calls"]) for f in record["figures"] for a in f)
    status = "ERROR " + error.strip().splitlines()[-1] if error else "ok"
    summary.append({"cell": cell_no, "figures": len(figs), "calls": n_calls, "status": status})
    print(f"{out_name:16s} cell {cell_no:2d}: {len(figs)} fig, {n_calls:3d} calls, {status}")

write_json(os.path.join(out_dir, "summary.json"), summary)
