"""Layer 1: typed per-file table from get_parsed_data_from_file (before derived columns).

Output: golden/parsed/<log>.json (column-oriented, see common.dataframe_to_json)
"""
import os
import sys

from common import DATASETS, GOLDEN, REPO, dataframe_to_json, log_files, write_json

sys.path.insert(0, REPO)
from src.read_log_file import get_parsed_data_from_file  # noqa: E402

for name in log_files():
    df = get_parsed_data_from_file(os.path.join(DATASETS, name))
    write_json(os.path.join(GOLDEN, "parsed", name + ".json"), dataframe_to_json(df))
    print(f"parsed    {name:42s} {len(df):6d} rows x {len(df.columns)} cols")
