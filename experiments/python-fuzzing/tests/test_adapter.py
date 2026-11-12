# Copyright (c) 2026 kiryl contributors
"""Real pytest invocations: binding, scope resets, selection and classification."""

from operator import attrgetter
from pathlib import Path  # noqa: TC003 -- Hypothesis introspects test annotations
from unittest.mock import patch

import hypothesis
import pytest
from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

from adapter import CaseResult, Invocation, run_session
from model import PytestPhase, ResultKind


@pytest.mark.parametrize(("scope", "count"), [("function", 3), ("module", 1), ("session", 1)])
def test_scopes_and_parameter_binding(scopes: Path, scope: str, count: int) -> None:
    """Reset only at invocation boundaries and bind each parameter variant."""
    # Resetting per input, retaining function fixtures across invocations, or
    # binding variant b to a's values each breaks a different assertion here.
    a, b = "test_scopes.py::test_bound[a]", "test_scopes.py::test_bound[b]"
    evidence = run_session(
        [str(scopes), "-q", "-k", "test_bound"],
        [Invocation(a, (b"", b"")), Invocation(a, (b"",)), Invocation(b, (b"",))],
    )
    assert set(evidence.selected) == {a, b}
    assert list(map(attrgetter("kind"), evidence.cases)) == [ResultKind.PASS] * 4
    assert evidence.fixture_events.count((scope, PytestPhase.SETUP)) == count
    assert evidence.fixture_events.count((scope, PytestPhase.TEARDOWN)) == count


@pytest.mark.parametrize(
    ("node", "kind"),
    [
        ("test_scopes.py::TestMethods::test_method", ResultKind.PASS),
        ("test_scopes.py::test_expected", ResultKind.PASS),
        ("test_scopes.py::test_reject", ResultKind.REJECT),
    ],
)
def test_methods_expected_errors_and_rejection(scopes: Path, node: str, kind: ResultKind) -> None:
    """Classify each supported method, expected exception, or rejected input."""
    evidence = run_session([str(scopes), "-q"], [Invocation(node, (b"",))])
    assert list(map(attrgetter("kind"), evidence.cases)) == [kind]


def test_setup_error_has_no_generated_case(scopes: Path) -> None:
    """Avoid blaming an input for a fixture that never reached test execution."""
    evidence = run_session(
        [str(scopes), "-q"], [Invocation("test_scopes.py::test_bad_setup", (b"",))]
    )
    assert len(evidence.cases) == 1
    assert evidence.cases[0].kind == ResultKind.INFRASTRUCTURE
    assert evidence.cases[0].replay_token is None


def test_empty_supported_selection_fails(scopes: Path) -> None:
    """Do not report a successful search when selection has no fuzz targets."""
    with pytest.raises(ValueError, match="Hypothesis"):
        run_session([str(scopes), "-k", "test_fixed", "-q"], [])


class FeedbackDriver:
    """Stream exactly one candidate and remember its observed feedback."""

    def __init__(self) -> None:
        """Start with no observed candidates."""
        self.seen: list[ResultKind] = []

    def next_case(self, nodeid: str) -> bytes | None:
        """Stop requesting candidates after the first feedback arrives."""
        assert nodeid == "test_scopes.py::test_expected"
        return b"" if not self.seen else None

    def observe(self, result: CaseResult) -> None:
        """Record immediate feedback from the adapter."""
        self.seen.append(result.kind)


def test_driver_streams_feedback_between_cases(scopes: Path) -> None:
    """Deliver feedback before requesting the next candidate from Rust."""
    node = "test_scopes.py::test_expected"

    driver = FeedbackDriver()
    evidence = run_session([str(scopes), "-q"], [Invocation(node, ())], driver)
    assert driver.seen == [ResultKind.PASS]
    assert len(evidence.cases) == 1


def test_method_instance_is_new_per_invocation(scopes: Path) -> None:
    """Catch class-instance state accidentally retained across time slices."""
    node = "test_scopes.py::TestMethods::test_fresh_instance"
    evidence = run_session([str(scopes), "-q"], [Invocation(node, (b"",))] * 2)
    assert list(map(attrgetter("kind"), evidence.cases)) == [ResultKind.PASS, ResultKind.PASS]


def test_unknown_selected_target_is_an_error(scopes: Path) -> None:
    """Reject stale IDs rather than silently searching a different test."""
    with pytest.raises(ValueError, match="unknown selected"):
        run_session([str(scopes), "-q"], [Invocation("missing", (b"",))])


def test_collection_failure_is_not_empty_selection(tmp_path: Path) -> None:
    """Preserve import errors instead of misclassifying them as empty selection."""
    target = tmp_path / "test_bad_import.py"
    target.write_text("raise RuntimeError('collection failed')")
    with pytest.raises(RuntimeError, match="collection"):
        run_session([str(target), "-q"], [])


def bound_invocation(tag: str) -> Invocation:
    """Translate a generated parameter schedule entry into one invocation."""
    return Invocation(f"test_scopes.py::test_bound[{tag}]", (b"", b""))


@settings(
    max_examples=12,
    deadline=None,
    suppress_health_check=[HealthCheck.function_scoped_fixture, HealthCheck.nested_given],
)
@given(tags=st.lists(st.sampled_from(["a", "b"]), min_size=1, max_size=8))
def test_parameter_invocation_sequences_do_not_leak_fixture_state(
    scopes: Path,
    tags: list[str],
) -> None:
    """Exercise arbitrary parameter schedules against real fixture lifecycles."""
    evidence = run_session(
        [str(scopes), "-q"],
        list(map(bound_invocation, tags)),
    )
    assert list(map(attrgetter("kind"), evidence.cases)) == [ResultKind.PASS] * (2 * len(tags))
    assert evidence.fixture_events.count(("function", PytestPhase.SETUP)) == len(tags)
    assert evidence.fixture_events.count(("function", PytestPhase.TEARDOWN)) == len(tags)


def test_unsupported_hypothesis_version_is_infrastructure(
    scopes: Path,
) -> None:
    """Reject version drift before it can become a misleading target finding."""
    with patch.object(hypothesis, "__version__", "unsupported"):
        evidence = run_session(
            [str(scopes), "-q"], [Invocation("test_scopes.py::test_expected", (b"",))]
        )
    assert list(map(attrgetter("kind"), evidence.cases)) == [ResultKind.INFRASTRUCTURE]
    assert evidence.cases[0].rendered is not None
    assert "unsupported Hypothesis" in evidence.cases[0].rendered


def test_async_property_is_explicitly_unsupported(tmp_path: Path) -> None:
    """Deferred async support cannot masquerade as a successful/failing sync oracle."""
    (tmp_path / "pytest.ini").write_text("[pytest]\n")
    (tmp_path / "test_async.py").write_text(
        "from hypothesis import given, strategies as st\n"
        "@given(st.just(0))\nasync def test_async(value):\n    assert False\n"
    )
    evidence = run_session([str(tmp_path), "-q"], [Invocation("test_async.py::test_async", (b"",))])
    assert evidence.cases[0].kind == ResultKind.INFRASTRUCTURE
    assert "async" in (evidence.cases[0].rendered or "")


@pytest.mark.parametrize("exception", ["InvalidArgument", "FailedHealthCheck"])
def test_hypothesis_configuration_exception_is_infrastructure(
    tmp_path: Path, exception: str
) -> None:
    """Framework configuration errors cannot become application counterexamples."""
    (tmp_path / "pytest.ini").write_text("[pytest]\n")
    (tmp_path / f"test_cfg_{exception}.py").write_text(
        "from hypothesis import given, strategies as st\n"
        f"from hypothesis.errors import {exception}\n"
        "@given(st.just(0))\ndef test_configuration(value):\n"
        f"    raise {exception}('synthetic framework error')\n"
    )
    evidence = run_session(
        [str(tmp_path), "-q"], [Invocation(f"test_cfg_{exception}.py::test_configuration", (b"",))]
    )
    assert evidence.cases[0].kind == ResultKind.INFRASTRUCTURE
