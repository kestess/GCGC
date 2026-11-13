#!/usr/bin/env bash
# Regenerate all golden outputs from the original Python code.
# Requires the venv: uv venv -p python3.9 .venv && uv pip install -p .venv/bin/python -r requirements-golden.txt
set -euo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
PY="$HERE/.venv/bin/python"
REPO="$(cd "$HERE/.." && pwd)"
cd "$HERE/tools"

rm -rf "$HERE/golden"
"$PY" regex_captures.py
"$PY" dump_parsed.py
MPLBACKEND=Agg "$PY" run_notebook.py "$REPO/src/notebooks/GCGC.ipynb" GCGC
MPLBACKEND=Agg "$PY" run_notebook.py "$REPO/src/notebooks/FeatureExamples.ipynb" FeatureExamples
MPLBACKEND=Agg "$PY" run_notebook.py "$REPO/src/tutorials/universal_plotting.ipynb" universal_plotting
du -sh "$HERE/golden"
