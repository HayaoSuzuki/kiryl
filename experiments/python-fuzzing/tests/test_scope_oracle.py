# Copyright (c) 2026 kiryl contributors
"""Same-premise correspondence for normal fixture returns, not proof of pytest."""

from operator import attrgetter
from pathlib import Path

import pytest

from adapter import Invocation, run_session
from model import PytestPhase, ResultKind


@pytest.mark.parametrize(
    ("count", "starts", "finishes"),
    [
        tuple(map(int, line.split("\t")))
        for line in (Path(__file__).parents[1] / "formal" / "scope-cases.tsv")
        .read_text()
        .splitlines()
    ],
)
def test_real_fixture_returns_match_lean_generated_counts(
    scopes: Path, count: int, starts: int, finishes: int
) -> None:
    """Compare one real observation with expectations generated exclusively by Lean."""
    evidence = run_session(
        [str(scopes), "-q", "-k", "test_bound"],
        [Invocation("test_scopes.py::test_bound[a]", (b"", b""))] * count,
    )
    assert evidence.fixture_events.count(("function", PytestPhase.SETUP)) == starts
    assert evidence.fixture_events.count(("function", PytestPhase.TEARDOWN)) == finishes
    assert len(evidence.cases) == 2 * count
    assert list(map(attrgetter("kind"), evidence.cases)) == [ResultKind.PASS] * (2 * count)
