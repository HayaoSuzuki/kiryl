# Copyright (c) 2026 kiryl contributors
"""Replay and semantic reduction against ordinary and stateful real tests."""

import json
import subprocess
import sys
import time
from collections.abc import Iterator  # noqa: TC003 -- pytest inspects annotations
from operator import attrgetter
from pathlib import Path

import pytest
from adapter import Invocation, minimize, replay, run_session
from hypothesis import given, settings
from hypothesis import strategies as st
from hypothesis_bridge import unpack_token
from model import ResultKind, Verification


@pytest.fixture
def finding_project(tmp_path: Path) -> Iterator[Path]:
    """Create an independent test project with two known, shrinkable defects."""
    (tmp_path / "pytest.ini").write_text("[pytest]\n")
    (tmp_path / "test_findings.py").write_text("""
import json
import pytest
import unittest
from pathlib import Path
from hypothesis import given, strategies as st, settings, Phase
from hypothesis.database import DirectoryBasedExampleDatabase
from hypothesis.stateful import RuleBasedStateMachine, rule, invariant

root = Path(__file__).parent
db = DirectoryBasedExampleDatabase(root / "db")

@settings(database=db, deadline=None, phases=[Phase.reuse])
@given(n=st.integers(min_value=0, max_value=100))
def test_integer(n):
    (root / "integer.json").write_text(json.dumps(n))
    assert n < 20

@settings(database=None, deadline=None)
@given(n=st.integers(min_value=0, max_value=100))
def test_no_database(n):
    assert n < 20

@settings(database=None, deadline=None)
@given(n=st.integers(min_value=0, max_value=100))
def test_competing(n):
    if n < 10:
        raise ValueError("different failure")
    assert n < 20

@pytest.mark.parametrize("tag", ["a", "b"])
@settings(database=db, deadline=None, phases=[Phase.reuse])
@given(n=st.integers(min_value=0, max_value=100))
def test_variant(tag, n):
    assert n < (20 if tag == "a" else 80)

@settings(database=None, deadline=None)
@given(n=st.just(1))
def test_unstable(n):
    marker = root / "once"
    if not marker.exists():
        marker.touch()
        assert False

class Accumulator(RuleBasedStateMachine):
    def __init__(self):
        super().__init__()
        self.values = []

    @rule(value=st.integers(min_value=0, max_value=100))
    def push(self, value):
        self.values.append(value)

    @invariant()
    def bounded(self):
        total = sum(self.values)
        if total >= 20:
            (root / "stateful.json").write_text(json.dumps(self.values))
        assert total < 20

TestAccumulator = Accumulator.TestCase
TestAccumulator.settings = settings(
    database=db, deadline=None, stateful_step_count=10, phases=[Phase.reuse]
)

class TestUnit(unittest.TestCase):
    def setUp(self):
        self.ready = True

    def tearDown(self):
        (root / "unit_teardown").touch()

    @settings(database=None, deadline=None)
    @given(n=st.just(1))
    def test_property(self, n):
        assert self.ready
        assert n == 1
""")
    yield tmp_path
    sys.modules.pop("test_findings", None)


def discover(root: Path, name: str) -> bytes:
    """Exercise known raw candidates for bridge correctness, not discovery scores."""
    node = f"test_findings.py::{name}"
    evidence = run_session(
        [str(root / "test_findings.py"), "-q"],
        [Invocation(node, tuple(bytes([value]) * 2048 for value in range(99, 120)))],
    )
    failures = [case for case in evidence.cases if case.kind == ResultKind.FAILURE]
    assert failures, evidence
    assert failures[0].replay_token is not None
    return failures[0].replay_token


def test_integer_semantic_shrinking_preserves_failure(finding_project: Path) -> None:
    """Shrink the semantic integer, retaining a replayable original and identity."""
    node = "test_findings.py::test_integer"
    args = [str(finding_project / "test_findings.py"), "-q"]
    original = discover(finding_project, "test_integer")
    before_value = json.loads((finding_project / "integer.json").read_text())
    observed = replay(node, original, args)
    assert observed.database_replay is True
    published: list[bytes] = []
    shrunk = minimize(node, original, 5.0, published.append)
    assert shrunk.original == original
    assert shrunk.complete is True
    assert published
    checked = replay(node, shrunk.best, args)
    assert checked.kind == ResultKind.FAILURE
    assert checked.fingerprint == observed.fingerprint
    assert shrunk.rendered is not None
    assert json.loads((finding_project / "integer.json").read_text()) < before_value


def test_stateful_semantic_shrinking(finding_project: Path) -> None:
    """Reproduce invariant failures and simplify operation arguments, not raw size."""
    node = "test_findings.py::TestAccumulator::runTest"
    args = [str(finding_project / "test_findings.py"), "-q"]
    original = discover(finding_project, "TestAccumulator::runTest")
    before = json.loads((finding_project / "stateful.json").read_text())
    shrunk = minimize(node, original, 5.0, lambda _: None)
    assert replay(node, shrunk.best, args).kind == ResultKind.FAILURE
    after = json.loads((finding_project / "stateful.json").read_text())
    assert (len(after), sum(after)) < (len(before), sum(before))


@pytest.mark.parametrize("name", ["test_integer", "TestAccumulator::runTest"])
def test_ordinary_pytest_reuses_database(finding_project: Path, name: str) -> None:
    """Persist Hypothesis choices under the original test identity for pytest."""
    discover(finding_project, name)
    completed = subprocess.run(  # noqa: S603 -- owned synthetic target and interpreter
        [
            sys.executable,
            "-m",
            "pytest",
            f"{finding_project / 'test_findings.py'}::{name}",
            "-q",
        ],
        capture_output=True,
        text=True,
        timeout=10,
        check=False,
    )
    assert completed.returncode == 1
    assert "AssertionError" in completed.stdout
    assert any(map(Path.is_file, (finding_project / "db").rglob("*")))


def test_no_database_and_zero_budget(finding_project: Path) -> None:
    """Explicit replay works without a DB; exhausted shrinking keeps the original."""
    node = "test_findings.py::test_no_database"
    original = discover(finding_project, "test_no_database")
    shrunk = minimize(node, original, 0.0, lambda _: None)
    assert shrunk.original == original
    assert shrunk.best == original
    assert shrunk.complete is False
    checked = replay(node, original, [str(finding_project / "test_findings.py"), "-q"])
    assert checked.kind == ResultKind.FAILURE
    assert checked.database_replay is False


def test_incompatible_token_is_rejected() -> None:
    """Reject unknown versions without interpreting arbitrary stored payloads."""
    with pytest.raises(ValueError, match="token"):
        replay("test_findings.py::test_integer", b'{"version":999}', [])


def test_database_parameter_variants_are_isolated(finding_project: Path) -> None:
    """Replay only the saved parameter variant with ordinary generation disabled."""
    discover(finding_project, "test_variant[b]")
    completed = subprocess.run(  # noqa: S603 -- owned target and interpreter
        [
            sys.executable,
            "-m",
            "pytest",
            str(finding_project / "test_findings.py"),
            "-k",
            "test_variant",
            "-q",
        ],
        capture_output=True,
        text=True,
        timeout=10,
        check=False,
    )
    assert completed.returncode == 1
    assert "1 failed, 1 skipped" in completed.stdout


def test_replay_of_state_dependent_failure_is_unstable(finding_project: Path) -> None:
    """Keep the observed failure separate from a successful fresh-context replay."""
    original = discover(finding_project, "test_unstable")
    checked = replay(
        "test_findings.py::test_unstable",
        original,
        [str(finding_project / "test_findings.py"), "-q"],
    )
    assert checked.kind == ResultKind.PASS
    assert checked.verification == Verification.UNSTABLE
    shrunk = minimize("test_findings.py::test_unstable", original, 1.0, lambda _: None)
    assert shrunk.best == original
    assert shrunk.verification == Verification.UNSTABLE


def test_acknowledgment_time_counts_against_shrink_budget(finding_project: Path) -> None:
    """Do not execute an input after its pre-execution acknowledgment used the budget."""
    original = discover(finding_project, "test_integer")
    witness = finding_project / "integer.json"
    witness.unlink()
    minimized = minimize(
        "test_findings.py::test_integer", original, 0.01, lambda _: time.sleep(0.03)
    )
    assert minimized.complete is False
    assert not witness.exists()


def test_shrinking_does_not_switch_to_a_competing_failure(finding_project: Path) -> None:
    """Keep the original assertion even when smaller inputs raise another error."""
    node = "test_findings.py::test_competing"
    args = [str(finding_project / "test_findings.py"), "-q"]
    original = discover(finding_project, "test_competing")
    observed = replay(node, original, args)
    shrunk = minimize(node, original, 5.0, lambda _: None)
    checked = replay(node, shrunk.best, args)
    assert checked.fingerprint == observed.fingerprint
    assert checked.verification == Verification.CONFIRMED


@settings(max_examples=75, deadline=None)
@given(
    version=st.one_of(
        st.integers(max_value=0),
        st.integers(min_value=2),
        st.booleans(),
        st.just(1.0),
        st.text(max_size=20),
    )
)
def test_wrong_schema_versions_never_reach_execution(version: object) -> None:
    """Reject bool/float coercion and arbitrary version fields, before loading code."""
    token = json.dumps(
        {
            "version": version,
            "hypothesis": "6.170.13",
            "python": list(sys.version_info[:2]),
            "nodeid": "unused",
            "pytest_args": [],
            "kind": "raw",
            "payload": "",
        }
    ).encode()
    with pytest.raises(ValueError, match="token"):
        unpack_token(token)


def test_unittest_setup_and_teardown_are_preserved(finding_project: Path) -> None:
    """Do not bypass the receiver's unittest lifecycle when invoking a property."""
    evidence = run_session(
        [str(finding_project / "test_findings.py"), "-q"],
        [Invocation("test_findings.py::TestUnit::test_property", (b"",))],
    )
    assert list(map(attrgetter("kind"), evidence.cases)) == [ResultKind.PASS]
    assert (finding_project / "unit_teardown").exists()


@pytest.mark.parametrize("name", ["test_integer", "TestAccumulator::runTest"])
def test_measured_ordinary_path_reuses_original_database(finding_project: Path, name: str) -> None:
    """Measurement leaves ordinary/stateful reuse-only Hypothesis identities intact."""
    from adapter import run_ordinary  # noqa: PLC0415 -- exercise the optional comparison path

    discover(finding_project, name)
    observations = []
    started = []
    code, _ = run_ordinary(
        [f"{finding_project / 'test_findings.py'}::{name}", "-q"],
        started.append,
        observations.append,
    )
    assert code == 1
    assert started
    assert ResultKind.FAILURE in set(map(attrgetter("kind"), observations))
