# Copyright (c) 2026 kiryl contributors
"""Own the product runtime admission rule before any project code is imported."""

import sys

HYPOTHESIS_VERSION = "6.170.13"
SUPPORTED_PYTEST_VERSIONS = ("9.1.1", "9.2.0")


def validate_runtime() -> None:
    """Reject unsupported interpreters and dependencies before collection/dispatch.

    Call only after control streams and resource guards have been reserved.
    Dependency imports are intentionally deferred for isolated worker startup.
    """
    if (
        sys.implementation.name != "cpython"
        or sys.version_info[:2] not in ((3, 14), (3, 15))
        or not sys._is_gil_enabled()  # noqa: SLF001 -- CPython capability
    ):
        message = "unsupported Python runtime: requires GIL CPython 3.14 or 3.15"
        raise RuntimeError(message)
    import hypothesis  # noqa: PLC0415 -- after stream/resource reservation
    import pytest  # noqa: PLC0415 -- after stream/resource reservation

    if (
        hypothesis.__version__ != HYPOTHESIS_VERSION
        or pytest.__version__ not in SUPPORTED_PYTEST_VERSIONS
    ):
        message = "unsupported pytest/Hypothesis runtime"
        raise RuntimeError(message)
