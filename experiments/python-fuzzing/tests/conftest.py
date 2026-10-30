# Copyright (c) 2026 kiryl contributors
"""Authored targets with real pytest fixtures for adapter correspondence tests."""

import sys
from collections.abc import Iterator  # noqa: TC003 -- fixture annotations are introspected
from pathlib import Path  # noqa: TC003 -- fixture annotations are introspected

import pytest


@pytest.fixture
def scopes(tmp_path: Path) -> Iterator[Path]:
    """Provide real fixture scopes and deliberate target/setup failure cases."""
    # This is its own project even when a mutation worker puts TMPDIR below
    # its root; inherited root configuration must not change the specimen IDs.
    (tmp_path / "pytest.ini").write_text("[pytest]\n")
    target = tmp_path / "test_scopes.py"
    target.write_text("""
import pytest
from adapter import FIXTURE_EVENTS
from hypothesis import given, strategies as st, assume, settings, HealthCheck

@pytest.fixture(scope="session")
def session(request):
    request.config.stash[FIXTURE_EVENTS].append(("session", "setup"))
    yield object()
    request.config.stash[FIXTURE_EVENTS].append(("session", "teardown"))

@pytest.fixture(scope="module")
def module(request, session):
    request.config.stash[FIXTURE_EVENTS].append(("module", "setup"))
    yield object()
    request.config.stash[FIXTURE_EVENTS].append(("module", "teardown"))

@pytest.fixture
def value(request, module):
    request.config.stash[FIXTURE_EVENTS].append(("function", "setup"))
    yield []
    request.config.stash[FIXTURE_EVENTS].append(("function", "teardown"))

@pytest.mark.parametrize("tag", ["a", "b"])
@settings(suppress_health_check=[HealthCheck.function_scoped_fixture])
@given(n=st.just(1))
def test_bound(value, tag, n):
    assert value in ([], [tag])
    value.append(tag)
    assert n == 1

class TestMethods:
    @settings(suppress_health_check=[HealthCheck.function_scoped_fixture])
    @given(n=st.just(1))
    def test_method(self, value, n):
        assert type(self).__name__ == "TestMethods"
        assert n == 1

    @given(n=st.just(1))
    def test_fresh_instance(self, n):
        self.calls = getattr(self, "calls", 0) + 1
        assert self.calls == 1

@given(n=st.just(1))
def test_expected(n):
    with pytest.raises(ValueError):
        raise ValueError("expected")

@given(n=st.just(1))
def test_reject(n):
    assume(False)

@pytest.fixture
def broken():
    raise RuntimeError("setup failed")

@given(n=st.just(1))
def test_bad_setup(broken, n):
    assert False

def test_fixed():
    assert False, "must not execute fixed tests"
""")
    yield target
    # Repeated pytest.main calls are a test harness concern; real workers own
    # one session. Prevent same-basename temporary specimens contaminating it.
    sys.modules.pop("test_scopes", None)
