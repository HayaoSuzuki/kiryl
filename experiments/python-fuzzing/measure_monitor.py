# Copyright (c) 2026 kiryl contributors
"""Bounded paired adapter timing; not a discovery benchmark or speed guarantee."""

import contextlib
import json
import sys
import tempfile
import time
from pathlib import Path

from adapter import Invocation, run_session
from model import ResultKind


def main() -> None:
    """Measure the same authored property with coverage disabled and enabled."""
    cases = 2000
    timings: dict[str, int] = {}
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        (root / "pytest.ini").write_text("[pytest]\n")
        (root / "timing_app.py").write_text(
            "def classify(n):\n    if n:\n        return 1\n    return 0\n"
        )
        target = root / "test_timing.py"
        target.write_text("""
from hypothesis import given, strategies as st
from timing_app import classify
@given(n=st.integers(min_value=0, max_value=1))
def test_property(n):
    assert classify(n) == n
""")
        for enabled in (False, True):
            start = time.perf_counter_ns()
            with contextlib.redirect_stdout(sys.stderr):
                evidence = run_session(
                    [str(target), "-q"],
                    [
                        Invocation(
                            "test_timing.py::test_property",
                            tuple(bytes([index % 2]) for index in range(cases)),
                        )
                    ],
                    collect_coverage=enabled,
                )
            elapsed = time.perf_counter_ns() - start
            if len(evidence.cases) != cases or any(
                case.kind != ResultKind.PASS for case in evidence.cases
            ):
                message = "timing specimen did not complete all cases"
                raise RuntimeError(message)
            timings["monitored_ns" if enabled else "direct_ns"] = elapsed
    sys.stdout.write(json.dumps({"cases": cases, **timings}) + "\n")


if __name__ == "__main__":
    main()
