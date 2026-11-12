# Copyright (c) 2026 kiryl contributors
"""Admission checks run before project collection or dispatch."""

import sys
from types import SimpleNamespace
from unittest.mock import create_autospec, patch

import hypothesis
import pytest
from runtime import validate_runtime


@pytest.mark.parametrize(
    ("attribute", "value"),
    [
        ("implementation", SimpleNamespace(name="pypy")),
        ("version_info", (3, 13)),
    ],
)
def test_unsupported_interpreter_is_rejected(attribute: str, value: object) -> None:
    """Each interpreter capability is independently required."""
    with (
        patch.object(sys, attribute, value),
        pytest.raises(RuntimeError, match="requires GIL CPython"),
    ):
        validate_runtime()


@pytest.mark.parametrize("dependency", [hypothesis, pytest])
def test_dependency_version_drift_is_rejected(dependency: object) -> None:
    """Either pinned framework version drifting invalidates the runtime."""
    with (
        patch.object(dependency, "__version__", "unsupported"),
        pytest.raises(RuntimeError, match="pytest/Hypothesis"),
    ):
        validate_runtime()


def test_supported_runtime_is_admitted() -> None:
    """The pinned interpreter and frameworks pass admission."""
    validate_runtime()


def test_disabled_gil_is_rejected() -> None:
    """A free-threaded interpreter cannot pass the runtime admission check."""
    gil_enabled = create_autospec(sys._is_gil_enabled, spec_set=True, return_value=False)  # noqa: SLF001 -- CPython capability admission boundary
    with (
        patch.object(sys, "_is_gil_enabled", gil_enabled),
        pytest.raises(RuntimeError, match="requires GIL CPython"),
    ):
        validate_runtime()
