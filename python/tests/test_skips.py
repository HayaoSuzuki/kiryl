# Copyright (c) 2026 kiryl contributors
"""Skipped invocations never become findings or useful passing coverage."""

import sys
from collections.abc import Iterator  # noqa: TC003 -- fixture annotation
from operator import attrgetter
from pathlib import Path  # noqa: TC003 -- pytest fixture annotation introspection

import pytest
from adapter import Invocation, replay, run_ordinary, run_session
from model import ResultKind, Verification


@pytest.fixture
def skip_project(tmp_path: Path) -> Iterator[Path]:
    """Exercise both skip families in bodies, markers and fixture setup."""
    (tmp_path / "pytest.ini").write_text("[pytest]\n")
    (tmp_path / "test_skip.py").write_text("""
import unittest
import pytest
from hypothesis import given, strategies as st, settings, HealthCheck

@pytest.fixture
def skip_setup():
    pytest.skip("fixture")

@settings(database=None)
@given(st.just(1))
def test_unit(value):
    raise unittest.SkipTest("unit")

@settings(database=None)
@given(st.just(1))
def test_pytest(value):
    pytest.skip("pytest")

@pytest.mark.skip(reason="marker")
@settings(database=None)
@given(st.just(1))
def test_marked(value):
    assert False

@settings(database=None, suppress_health_check=[HealthCheck.function_scoped_fixture])
@given(st.just(1))
def test_setup(skip_setup, value):
    assert False

@settings(database=None)
@given(st.just(1))
def test_later_skip(value):
    from pathlib import Path
    if Path(__file__).with_suffix(".marker").exists():
        pytest.skip("changed circumstances")
    assert False

@settings(database=None)
@given(st.just(1))
def test_later_unit(value):
    from pathlib import Path
    if Path(__file__).with_suffix(".marker").exists():
        raise unittest.SkipTest("changed circumstances")
    assert False

@pytest.fixture(params=["pytest", "unit"])
def later_setup(request):
    from pathlib import Path
    if Path(__file__).with_suffix(".marker").exists():
        if request.param == "unit":
            raise unittest.SkipTest("fixture")
        pytest.skip("fixture")

@settings(database=None, suppress_health_check=[HealthCheck.function_scoped_fixture])
@given(st.just(1))
def test_later_setup(later_setup, value):
    assert False

""")
    yield tmp_path
    sys.modules.pop("test_skip", None)


@pytest.mark.parametrize("name", ["unit", "pytest", "marked", "setup"])
def test_search_skip_is_explicit(skip_project: Path, name: str) -> None:
    """Every supported skip boundary yields one skipped invocation, never failure."""
    node = f"test_skip.py::test_{name}"
    evidence = run_session([str(skip_project), "-q"], [Invocation(node, (b"",))])
    assert list(map(attrgetter("kind"), evidence.cases)) == ["skip"]
    assert set(map(attrgetter("replay_token"), evidence.cases)) == {None}


@pytest.mark.parametrize("name", ["unit", "pytest", "marked", "setup"])
def test_ordinary_skip_is_not_pass(skip_project: Path, name: str) -> None:
    """Native pytest still skips and telemetry never records passing coverage."""
    observed = []
    code, _ = run_ordinary(
        [str(skip_project), "-q", "-k", f"test_{name}"], lambda _: None, observed.append
    )
    assert code == 0
    assert list(map(attrgetter("kind"), observed)) == ["skip"]


@pytest.mark.parametrize(
    "name", ["later_skip", "later_unit", "later_setup[pytest]", "later_setup[unit]"]
)
def test_replay_skip_is_unstable(skip_project: Path, name: str) -> None:
    """A property skipping in the fresh environment cannot confirm its original."""
    node = f"test_skip.py::test_{name}"
    args = [str(skip_project), "-q"]
    original = run_session(args, [Invocation(node, (b"",))]).cases[0]
    assert original.replay_token is not None
    (skip_project / "test_skip.marker").touch()
    result = replay(node, original.replay_token, args)
    assert result.kind == ResultKind.SKIP
    assert result.verification == Verification.UNSTABLE
