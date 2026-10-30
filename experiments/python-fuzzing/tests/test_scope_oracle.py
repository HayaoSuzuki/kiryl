# Copyright (c) 2026 kiryl contributors
"""Same-premise correspondence for normal fixture returns, not proof of pytest."""

from pathlib import Path

from adapter import Invocation, run_session


def test_real_fixture_returns_match_lean_generated_counts(scopes: Path) -> None:
    """Compare real observations with expectations generated exclusively by Lean."""
    corpus = Path(__file__).parents[1] / "formal" / "scope-cases.tsv"
    for line in corpus.read_text().splitlines():
        count, starts, finishes = map(int, line.split("\t"))
        evidence = run_session(
            [str(scopes), "-q", "-k", "test_bound"],
            [Invocation("test_scopes.py::test_bound[a]", (b"", b"")) for _ in range(count)],
        )
        assert evidence.fixture_events.count(("function", "setup")) == starts
        assert evidence.fixture_events.count(("function", "teardown")) == finishes
        assert len(evidence.cases) == 2 * count
        assert all(case.kind == "pass" for case in evidence.cases)
