#!/usr/bin/env python3
"""Compares two sniper-scene run files on every number that does not name a tx, receipt, key,
block height or time. Prints each difference. Exit 0 when the numbers are identical."""
import json
import sys

VOLATILE = {"tx_hash", "receipt_id", "window_end_ns", "block", "coin_receipt_block", "failed_receipt_block",
            "quote_block", "launch_block", "account", "launch_ns", "head_after_launch_returned", "seconds", "receipts", "events", "window_start_ns",
            "fee_end_block"}


def strip(x):
    if isinstance(x, dict):
        return {k: strip(v) for k, v in x.items() if k not in VOLATILE}
    if isinstance(x, list):
        return [strip(v) for v in x]
    return x


def diff(a, b, path=""):
    out = []
    if isinstance(a, dict) and isinstance(b, dict):
        for k in sorted(set(a) | set(b)):
            out += diff(a.get(k), b.get(k), f"{path}.{k}")
    elif isinstance(a, list) and isinstance(b, list) and len(a) == len(b):
        for i, (x, y) in enumerate(zip(a, b)):
            out += diff(x, y, f"{path}[{i}]")
    elif a != b:
        out.append(f"{path}: {a} != {b}")
    return out


a, b = (strip(json.load(open(p))) for p in sys.argv[1:3])
d = diff(a, b)
for line in d[:60]:
    print(line)
print(f"compare: {len(d)} differences")
sys.exit(1 if d else 0)
