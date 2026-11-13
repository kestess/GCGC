"""Layer 0: raw regex capture groups for every matching line of every dataset.

Uses only the stdlib `re` and the unmodified src/parse_log_file.py.
Output: golden/regex/{regex.txt, columns.json, captures/<log>.json}
Each capture row is [line_number, group1, ..., group30].
"""
import importlib.util
import json
import os
import re

from common import DATASETS, GOLDEN, REPO, log_files, write_json

spec = importlib.util.spec_from_file_location("plf", os.path.join(REPO, "src", "parse_log_file.py"))
plf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(plf)

rx, cols, types = plf.get_parsing_groups()
out = os.path.join(GOLDEN, "regex")
os.makedirs(out, exist_ok=True)
with open(os.path.join(out, "regex.txt"), "w") as fh:
    fh.write(rx)
write_json(os.path.join(out, "columns.json"), {"columns": cols, "types": [t.__name__ for t in types]})

cre = re.compile(rx)
for name in log_files():
    rows = []
    with open(os.path.join(DATASETS, name)) as fh:
        for ln, line in enumerate(fh):
            m = cre.search(line)
            if m:
                rows.append([ln] + list(m.groups()))
    write_json(os.path.join(out, "captures", name + ".json"), rows)
    print(f"regex     {name:42s} {len(rows):6d} rows")
