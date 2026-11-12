# Copyright (c) 2026 kiryl contributors
"""Check Linux per-worker crash reporting without changing host-wide policy."""

import ctypes
import errno
from types import SimpleNamespace
from typing import Protocol
from unittest.mock import call, create_autospec, patch

import pytest

import worker


class PrctlAPI(Protocol):
    """Declare Linux prctl's documented call signature and ctypes descriptors."""

    argtypes: object = None
    restype: object = None

    def __call__(self, option: int, arg2: int, arg3: int, arg4: int, arg5: int) -> int:
        """Invoke PR_SET_DUMPABLE using the Linux prctl argument contract."""
        ...


def test_linux_core_policy_disables_dumpability() -> None:
    """RLIMIT_CORE alone cannot disable a piped system core handler."""
    prctl = create_autospec(PrctlAPI, instance=True, spec_set=True, return_value=0)
    library = SimpleNamespace(prctl=prctl)
    load = create_autospec(ctypes.CDLL, spec_set=True, return_value=library)
    with patch.object(ctypes, "CDLL", load):
        worker.disable_linux_core_dumps()
    load.assert_called_once_with(None, use_errno=True)
    prctl.assert_called_once()
    assert prctl.call_args == call(4, 0, 0, 0, 0)
    assert prctl.argtypes == (ctypes.c_int,) + (ctypes.c_ulong,) * 4
    assert prctl.restype is ctypes.c_int


def test_failed_linux_core_policy_is_infrastructure() -> None:
    """An unavailable policy must not silently enable slow crash-reporting paths."""
    prctl = create_autospec(PrctlAPI, instance=True, spec_set=True, return_value=-1)
    load = create_autospec(ctypes.CDLL, spec_set=True, return_value=SimpleNamespace(prctl=prctl))
    get_errno = create_autospec(ctypes.get_errno, spec_set=True, return_value=errno.EPERM)
    with (
        patch.object(ctypes, "CDLL", load),
        patch.object(ctypes, "get_errno", get_errno),
        pytest.raises(OSError, match="PR_SET_DUMPABLE") as error,
    ):
        worker.disable_linux_core_dumps()
    assert error.value.errno == errno.EPERM
