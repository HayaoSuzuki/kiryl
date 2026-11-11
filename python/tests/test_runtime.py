# Copyright (c) 2026 kiryl contributors
"""Admission checks run before project collection or dispatch."""

import sys
from types import SimpleNamespace

import hypothesis
import pytest
from runtime import validate_runtime


@pytest.mark.parametrize(
    ("attribute", "value"),
    [
        ("implementation", SimpleNamespace(name="pypy")),
        ("version_info", (3, 13)),
        ("_is_gil_enabled", lambda: False),
    ],
)
def test_unsupported_interpreter_is_rejected(
    monkeypatch: pytest.MonkeyPatch, attribute: str, value: object
) -> None:
    """Each interpreter capability is independently required."""
    monkeypatch.setattr(sys, attribute, value)
    with pytest.raises(RuntimeError, match="requires GIL CPython"):
        validate_runtime()


@pytest.mark.parametrize("dependency", [hypothesis, pytest])
def test_dependency_version_drift_is_rejected(
    monkeypatch: pytest.MonkeyPatch, dependency: object
) -> None:
    """Either pinned framework version drifting invalidates the runtime."""
    monkeypatch.setattr(dependency, "__version__", "unsupported")
    with pytest.raises(RuntimeError, match="pytest/Hypothesis"):
        validate_runtime()


def test_supported_runtime_is_admitted() -> None:
    """The pinned interpreter and frameworks pass admission."""
    validate_runtime()
