# Copyright (c) 2026 kiryl contributors
"""Replay envelope boundaries exercised against real pytest-bound properties."""

import json
import sys
from collections.abc import Iterator  # noqa: TC003 -- pytest fixture annotation
from operator import attrgetter
from pathlib import Path  # noqa: TC003 -- pytest fixture annotation
from random import Random
from unittest.mock import create_autospec, patch

import adapter
import hypothesis
import hypothesis_bridge
import pytest
from adapter import Invocation, minimize, run_ordinary, run_session, run_token
from hypothesis.database import choices_to_bytes
from hypothesis.internal.conjecture.choice import ChoiceTemplate, ValueHole
from hypothesis.internal.conjecture.data import ConjectureData
from hypothesis_bridge import BoundTarget, bind_target, instrument_ordinary, unpack_token


@pytest.fixture
def bridge_project(tmp_path: Path) -> Iterator[Path]:
    """Provide passing and setup-skipped targets in an isolated pytest project."""
    (tmp_path / "pytest.ini").write_text("[pytest]\n")
    source = tmp_path / "test_bridge_edges.py"
    source.write_text(
        "import pytest, unittest, hypothesis\n"
        "from hypothesis import given, settings, strategies as st\n"
        "@settings(database=None)\n"
        "@given(value=st.just(0))\n"
        "def test_pass(value):\n    assert value == 0\n"
        "@pytest.mark.skip(reason='setup skip')\n"
        "@given(value=st.just(0))\n"
        "def test_skip(value):\n    assert value == 0\n"
        "def test_plain():\n    assert True\n"
        "class TestOwned(unittest.TestCase):\n"
        "    def __init__(self, *args, **kwargs):\n"
        "        super().__init__(*args, **kwargs)\n"
        "        self.__dict__[self._testMethodName] = getattr(\n"
        "            self, self._testMethodName, self.test_property)\n"
        "    @given(value=st.just(0))\n"
        "    def test_property(self, value):\n        assert value == 0\n"
        "class TestSkipped(unittest.TestCase):\n"
        "    def setUp(self):\n        self.skipTest('lifecycle skip')\n"
        "    @given(value=st.just(0))\n"
        "    def test_property(self, value):\n        assert value == 0\n"
        "class TestBroken(unittest.TestCase):\n"
        "    def setUp(self):\n        raise RuntimeError('lifecycle error')\n"
        "    @given(value=st.just(0))\n"
        "    def test_property(self, value):\n        assert value == 0\n"
        "@given(value=st.just(0))\n"
        "def test_failure(value):\n    assert value != 0\n"
        "@given(value=st.just(0))\n"
        "def test_rejection(value):\n    hypothesis.assume(False)\n"
        "calls = []\n"
        "@settings(database=None)\n"
        "@given(value=st.integers(0, 100))\n"
        "def test_changes_during_shrinking(value):\n"
        "    calls.append(value)\n    assert len(calls) > 2\n"
    )
    yield source
    sys.modules.pop("test_bridge_edges", None)


@pytest.fixture
def target(bridge_project: Path) -> BoundTarget:
    """Capture a real binding for the fixture-free constant property."""
    targets: list[BoundTarget] = []

    def capture(item: pytest.Function) -> None:
        targets.append(bind_target(item))

    adapter._in_context(  # noqa: SLF001 -- exercise the real context boundary without replacing it
        "test_bridge_edges.py::test_pass", [str(bridge_project), "-q"], capture
    )
    return targets[0]


@pytest.mark.parametrize(
    ("changes", "message"),
    [
        ({"nodeid": "other"}, "target mismatch"),
        ({"kind": "unknown"}, "unknown replay token kind"),
        ({"kind": "choices", "prefix": [{"choice": "ff"}]}, "invalid replay token choice"),
        ({"kind": "choices", "prefix": [{"choice": ""}]}, "invalid replay token choice"),
    ],
    ids=["foreign-target", "unknown-kind", "malformed-choice", "empty-choice"],
)
def test_invalid_replay_candidates_fail_before_evaluation(
    target: BoundTarget, changes: dict[str, object], message: str
) -> None:
    """Reject each independently malformed replay candidate before execution."""
    envelope = dict(unpack_token(target.token("raw", "")))
    envelope.update(changes)
    with pytest.raises(ValueError, match=message):
        target.check_token(json.dumps(envelope).encode())


@pytest.mark.parametrize(
    "prefix", [[], [{"template": 1}], [{"choice": choices_to_bytes((True,)).hex()}]]
)
def test_choice_prefix_replay_accepts_constant_property(
    target: BoundTarget, prefix: list[dict[str, object]]
) -> None:
    """Replay valid templates and concrete choices without altering the target."""
    token = target.token("choices", "", prefix=prefix, random=Random(0).getstate())  # noqa: S311 -- deterministic replay state
    assert target.check_token(token).kind == "pass"


@pytest.mark.parametrize("prefix", [(ChoiceTemplate("simplest", 1),), (True,)])
def test_choice_token_round_trip(target: BoundTarget, prefix: tuple[ChoiceTemplate | bool]) -> None:
    """Encode real Hypothesis prefixes and replay the resulting bridge token."""
    data = ConjectureData(random=Random(0), prefix=prefix)  # noqa: S311 -- deterministic replay
    token = target._choice_token(data)  # noqa: SLF001 -- pinned compatibility seam under test
    assert target.check_token(token).kind == "pass"


def test_opaque_choice_prefix_has_no_unsafe_replay_token(target: BoundTarget) -> None:
    """Reject strategy inversion holes rather than fabricating replay evidence."""
    data = ConjectureData(random=Random(0), prefix=(ValueHole(1),))  # noqa: S311 -- replay
    with pytest.raises(TypeError, match="no safe replay token"):
        target._choice_token(data)  # noqa: SLF001 -- pinned compatibility seam under test


def test_raw_token_callback_precedes_case_execution(
    bridge_project: Path, target: BoundTarget
) -> None:
    """Acknowledge the candidate before evaluating a fresh replay context."""
    acknowledged: list[bool] = []
    result = run_token(
        target.item.nodeid,
        target.token("raw", ""),
        [str(bridge_project), "-q"],
        lambda: acknowledged.append(True),
    )
    assert acknowledged == [True]
    assert result.kind == "pass"
    assert result.verification == "unstable"


def test_minimization_setup_skip_remains_skip(target: BoundTarget) -> None:
    """A skipped invocation cannot manufacture a semantic reduction result."""
    with pytest.raises(pytest.skip.Exception, match="invocation skipped"):
        minimize("test_bridge_edges.py::test_skip", target.token("raw", ""), 1.0, lambda _: None)


def test_invalid_context_selection_propagates_collection_error(
    bridge_project: Path, target: BoundTarget
) -> None:
    """A stale replay target fails before its property can run."""
    with pytest.raises(ValueError, match="unknown selected"):
        run_token("missing", target.token("raw", ""), [str(bridge_project), "-q"])


def test_context_operation_failure_is_infrastructure(
    bridge_project: Path, target: BoundTarget
) -> None:
    """Malformed raw bytes preserve the context operation's infrastructure error."""
    with pytest.raises(RuntimeError, match="fromhex"):
        run_token(target.item.nodeid, target.token("raw", "zz"), [str(bridge_project), "-q"])


def test_ordinary_non_hypothesis_target_is_left_to_pytest(bridge_project: Path) -> None:
    """The measurement plugin passes unrelated ordinary tests through unchanged."""
    outcomes: list[object] = []
    code, _ = run_ordinary(
        [str(bridge_project), "-q", "-k", "test_plain"], lambda _: None, outcomes.append
    )
    assert code == 0
    assert outcomes == []


def test_measurement_version_drift_is_infrastructure(bridge_project: Path) -> None:
    """Reject incompatible measurement internals before ordinary generation."""
    with patch.object(hypothesis, "__version__", "unsupported"):
        code, _ = run_ordinary(
            [str(bridge_project), "-q", "-k", "test_pass"], lambda _: None, lambda _: None
        )
    assert code == 1


def test_driver_retention_is_bounded_while_feedback_is_complete(bridge_project: Path) -> None:
    """Streaming campaigns retain bounded history while acknowledging every input."""
    driver = CountingDriver(260)
    evidence = run_session(
        [str(bridge_project), "-q"],
        [Invocation("test_bridge_edges.py::test_pass", ())],
        driver,
        collect_coverage=False,
    )
    assert driver.observed == driver.count
    assert len(evidence.cases) == adapter.MAX_RETAINED_CASES


class CountingDriver:
    """Provide one bounded stream and retain only its feedback count."""

    def __init__(self, count: int) -> None:
        """Start with a fixed number of acknowledged candidates."""
        self.count = count
        self.observed = 0

    def next_case(self, nodeid: str) -> bytes | None:
        """Stop requesting input once the prescribed count is acknowledged."""
        assert nodeid
        return b"" if self.observed < self.count else None

    def observe(self, result: adapter.CaseResult) -> None:
        """Count every completed candidate independently of retention."""
        assert result.kind in {"pass", "skip"}
        self.observed += 1


@pytest.mark.parametrize(
    ("name", "kind"),
    [("TestOwned", "pass"), ("TestSkipped", "skip"), ("TestBroken", "infrastructure")],
)
def test_unittest_instance_lifecycle_edges(bridge_project: Path, name: str, kind: str) -> None:
    """Respect instance-owned methods, unittest skips, and setup failures."""
    node = f"test_bridge_edges.py::{name}::test_property"
    evidence = run_session([str(bridge_project), "-q"], [Invocation(node, (b"",))])
    assert evidence.cases[0].kind == kind


def test_driver_receives_setup_skip(bridge_project: Path) -> None:
    """Send fixture-level skips to the driver without generating a candidate."""
    driver = CountingDriver(1)
    evidence = run_session(
        [str(bridge_project), "-q"], [Invocation("test_bridge_edges.py::test_skip", ())], driver
    )
    assert driver.observed == driver.count
    assert evidence.cases[0].kind == "skip"


def test_invalid_pytest_option_is_infrastructure(bridge_project: Path) -> None:
    """A usage error cannot be accepted as an empty successful campaign."""
    with pytest.raises(RuntimeError, match="pytest infrastructure exit"):
        run_session([str(bridge_project), "--kiryl-invalid-option"], [])


def test_measurement_rejects_version_before_wrapping(target: BoundTarget) -> None:
    """Guard the compatibility seam independently of pytest's own version checks."""
    with (
        patch.object(hypothesis, "__version__", "unsupported"),
        pytest.raises(RuntimeError, match="measurement version"),
    ):
        instrument_ordinary(target.item, lambda: None, lambda _: None)


class AdvancingClock:
    """Supply deterministic elapsed time across a minimization boundary."""

    def __init__(self, ticks: tuple[float, ...]) -> None:
        """Retain the finite observed clock sequence."""
        self.ticks = iter(ticks)

    def __call__(self) -> float:
        """Return the next observation, remaining expired after exhaustion."""
        return next(self.ticks, 100.0)


@pytest.mark.parametrize("ticks", [(0.0, 0.0, 2.0), (0.0, 0.0, 0.0, 2.0)])
def test_shrink_budget_expires_around_candidate_acknowledgement(
    bridge_project: Path, ticks: tuple[float, ...]
) -> None:
    """Expiry before or after candidate acknowledgement preserves confirmed evidence."""
    node = "test_bridge_edges.py::test_failure"
    evidence = run_session([str(bridge_project), "-q"], [Invocation(node, (b"",))])
    token = evidence.cases[0].replay_token
    assert token is not None
    clock = create_autospec(
        hypothesis_bridge.time.monotonic, spec_set=True, side_effect=AdvancingClock(ticks)
    )
    with patch.object(hypothesis_bridge.time, "monotonic", clock):
        result = minimize(node, token, 1.0, lambda _: None)
    assert result.complete is False
    assert result.original == token
    assert result.verification == "confirmed"


def test_measurement_rejects_changed_database_digest(target: BoundTarget) -> None:
    """Refuse instrumentation when wrapping changes persistent database identity."""
    digests = iter((b"wrapped", b"original"))
    digest = create_autospec(
        hypothesis_bridge.function_digest, spec_set=True, side_effect=lambda _: next(digests)
    )
    with (
        patch.object(hypothesis_bridge, "function_digest", digest),
        pytest.raises(RuntimeError, match="database identity"),
    ):
        instrument_ordinary(target.item, lambda: None, lambda _: None)


def test_ordinary_rejection_is_reported_before_unsatisfied_property_exit(
    bridge_project: Path,
) -> None:
    """The measurement callback retains rejected cases during native generation."""
    outcomes: list[adapter.CaseResult] = []
    code, _ = run_ordinary(
        [str(bridge_project), "-q", "-k", "test_rejection"], lambda _: None, outcomes.append
    )
    assert code == 1
    assert "reject" in list(map(attrgetter("kind"), outcomes))


def test_fresh_replay_collection_failure_is_infrastructure(
    bridge_project: Path, target: BoundTarget
) -> None:
    """A fresh interpreter's collection failure remains an infrastructure result."""
    result = adapter.replay(
        target.item.nodeid, target.token("raw", ""), [str(bridge_project.parent / "missing.py")]
    )
    assert result.kind == "infrastructure"
    assert "missing.py" in (result.rendered or "")


def suppress_test_protocol(project: Path) -> None:
    """Let a real pytest plugin claim the protocol without executing a test."""
    (project.parent / "conftest.py").write_text(
        "def pytest_runtest_protocol(item, nextitem):\n    return True\n"
    )


def test_plugin_suppressing_minimization_does_not_produce_a_result(
    bridge_project: Path, target: BoundTarget
) -> None:
    """External protocol suppression cannot invent minimization evidence."""
    suppress_test_protocol(bridge_project)
    with pytest.raises(RuntimeError, match="minimization did not execute"):
        minimize(target.item.nodeid, target.token("raw", ""), 1.0, lambda _: None)


def test_plugin_suppressing_replay_does_not_produce_a_result(
    bridge_project: Path, target: BoundTarget
) -> None:
    """External protocol suppression cannot invent replay evidence."""
    suppress_test_protocol(bridge_project)
    with pytest.raises(RuntimeError, match="replay did not execute"):
        run_token(target.item.nodeid, target.token("raw", ""), [str(bridge_project), "-q"])


def test_property_changing_during_shrinking_retains_uncompleted_original(
    bridge_project: Path,
) -> None:
    """A failure disappearing after initial confirmation produces no completed shrink."""
    node = "test_bridge_edges.py::test_changes_during_shrinking"
    evidence = run_session([str(bridge_project), "-q"], [Invocation(node, (b"\x00",))])
    token = evidence.cases[0].replay_token
    assert token is not None
    result = minimize(node, token, 1.0, lambda _: None)
    assert result.complete is False
    assert result.best == token
