"""Shared helpers for the golden-output dumpers."""
import json
import math
import os

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
DATASETS = os.path.join(REPO, "datasets")
GOLDEN = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "golden"))


def log_files():
    return sorted(f for f in os.listdir(DATASETS) if not f.endswith((".md", ".csv")))


def to_jsonable(v):
    """Convert numpy/pandas values to plain JSON. NaN -> null, +/-inf -> "inf"/"-inf"."""
    import numpy as np
    import pandas as pd

    if v is None or isinstance(v, (str, bool)):
        return v
    if isinstance(v, (np.bool_,)):
        return bool(v)
    if isinstance(v, (int, np.integer)):
        return int(v)
    if isinstance(v, (float, np.floating)):
        f = float(v)
        if math.isnan(f):
            return None
        if math.isinf(f):
            return "inf" if f > 0 else "-inf"
        return f
    if isinstance(v, (pd.Series, pd.Index)):
        return [to_jsonable(x) for x in v.tolist()]
    if isinstance(v, np.ndarray):
        return [to_jsonable(x) for x in v.tolist()]
    if isinstance(v, (list, tuple)):
        return [to_jsonable(x) for x in v]
    if isinstance(v, dict):
        return {str(k): to_jsonable(x) for k, x in v.items()}
    if isinstance(v, pd.DataFrame):
        return dataframe_to_json(v)
    # matplotlib Text objects, colors given as named strings etc.
    if hasattr(v, "get_text"):
        return v.get_text()
    return repr(v)


def dataframe_to_json(df):
    """Column-oriented dump that preserves column order, index and dtypes."""
    return {
        "columns": [str(c) for c in df.columns],
        "dtypes": {str(c): str(t) for c, t in df.dtypes.items()},
        "index": to_jsonable(df.index),
        "data": {str(c): to_jsonable(df[c]) for c in df.columns},
    }


def write_json(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as fh:
        json.dump(obj, fh, separators=(",", ":"), allow_nan=False)
