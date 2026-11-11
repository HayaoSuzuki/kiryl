# Copyright (c) 2026 kiryl contributors
"""Real pytest invocations: binding, scope resets, selection and classification."""

from pathlib import Path  # noqa: TC003 -- Hypothesis introspects test annotations

import hypothesis
import pytest
from adapter import CaseResult, Invocation, run_session
from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st


def test_scopes_and_parameter_binding(scopes: Path) -> None:
    """Reset only at invocation boundaries and bind each parameter variant."""
    # Resetting per input, retaining function fixtures across invocations, or
    # binding variant b to a's values each breaks a different assertion here.
    a, b = "test_scopes.py::test_bound[a]", "test_scopes.py::test_bound[b]"
    evidence = run_session(
        [str(scopes), "-q", "-k", "test_bound"],
        [Invocation(a, (b"", b"")), Invocation(a, (b"",)), Invocation(b, (b"",))],
    )
    assert evidence.selected == [a, b]
    assert [case.kind for case in evidence.cases] == ["pass"] * 4
    for scope, count in [("function", 3), ("module", 1), ("session", 1)]:
        assert evidence.fixture_events.count((scope, "setup")) == count
        assert evidence.fixture_events.count((scope, "teardown")) == count


def test_methods_expected_errors_and_rejection(scopes: Path) -> None:
    """Keep receiver binding and rejection independent of expected exceptions."""
    ids = [
        "test_scopes.py::TestMethods::test_method",
        "test_scopes.py::test_expected",
        "test_scopes.py::test_reject",
    ]
    evidence = run_session([str(scopes), "-q"], [Invocation(node, (b"",)) for node in ids])
    assert [case.kind for case in evidence.cases] == ["pass", "pass", "reject"]


def test_setup_error_has_no_generated_case(scopes: Path) -> None:
    """Avoid blaming an input for a fixture that never reached test execution."""
    evidence = run_session(
        [str(scopes), "-q"], [Invocation("test_scopes.py::test_bad_setup", (b"",))]
    )
    assert len(evidence.cases) == 1
    assert evidence.cases[0].kind == "infrastructure"
    assert evidence.cases[0].replay_token is None


def test_empty_supported_selection_fails(scopes: Path) -> None:
    """Do not report a successful search when selection has no fuzz targets."""
    with pytest.raises(ValueError, match="Hypothesis"):
        run_session([str(scopes), "-k", "test_fixed", "-q"], [])


def test_driver_streams_feedback_between_cases(scopes: Path) -> None:
    """Deliver feedback before requesting the next candidate from Rust."""
    node = "test_scopes.py::test_expected"

    class Driver:
        def __init__(self) -> None:
            self.seen: list[str] = []

        def next_case(self, nodeid: str) -> bytes | None:
            assert nodeid == node
            return b"" if not self.seen else None

        def observe(self, result: CaseResult) -> None:
            self.seen.append(result.kind)

    driver = Driver()
    evidence = run_session([str(scopes), "-q"], [Invocation(node, ())], driver)
    assert driver.seen == ["pass"]
    assert len(evidence.cases) == 1


def test_method_instance_is_new_per_invocation(scopes: Path) -> None:
    """Catch class-instance state accidentally retained across time slices."""
    node = "test_scopes.py::TestMethods::test_fresh_instance"
    evidence = run_session([str(scopes), "-q"], [Invocation(node, (b"",))] * 2)
    assert [case.kind for case in evidence.cases] == ["pass", "pass"]


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
        [Invocation(f"test_scopes.py::test_bound[{tag}]", (b"", b"")) for tag in tags],
    )
    assert [case.kind for case in evidence.cases] == ["pass"] * (2 * len(tags))
    assert evidence.fixture_events.count(("function", "setup")) == len(tags)
    assert evidence.fixture_events.count(("function", "teardown")) == len(tags)


def test_unsupported_hypothesis_version_is_infrastructure(
    scopes: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Reject version drift before it can become a misleading target finding."""
    monkeypatch.setattr(hypothesis, "__version__", "unsupported")
    evidence = run_session(
        [str(scopes), "-q"], [Invocation("test_scopes.py::test_expected", (b"",))]
    )
    assert [case.kind for case in evidence.cases] == ["infrastructure"]
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
    assert evidence.cases[0].kind == "infrastructure"
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
    assert evidence.cases[0].kind == "infrastructure"
