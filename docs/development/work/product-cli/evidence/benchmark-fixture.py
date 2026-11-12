# Copyright (c) 2026 kiryl contributors
"""Distributable planted branch exercise; all assertions express true identities."""
import atexit
import json
import os
from pathlib import Path

from hypothesis import given, strategies as st
from application import classify

calls = 0
paths = set()


def record():
    """Append one bounded session summary, rather than writing per trial."""
    if os.environ.get("KIRYL_BENCH_METRICS"):
        with Path(os.environ["KIRYL_BENCH_METRICS"]).open("a", encoding="utf-8") as output:
            output.write(json.dumps({"pid": os.getpid(), "calls": calls, "paths": sorted(paths)}) + "\n")


atexit.register(record)


@given(st.integers(min_value=-(2**31), max_value=2**31 - 1))
def test_integer_branches(value):
    """Exercise nested sign, parity, residue and magnitude decisions."""
    global calls
    calls += 1
    path = classify(value)
    paths.add(path)
    assert value == (value // 7) * 7 + value % 7
    assert abs(value) >= 0
