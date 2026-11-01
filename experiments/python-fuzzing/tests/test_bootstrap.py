# Copyright (c) 2026 kiryl contributors
"""Check Linux per-worker crash reporting without changing host-wide policy."""

import ctypes
import errno
from unittest.mock import Mock

import pytest

import worker


def test_linux_core_policy_disables_dumpability(monkeypatch: pytest.MonkeyPatch) -> None:
    """RLIMIT_CORE alone cannot disable a piped system core handler."""
    library = Mock()
    library.prctl.return_value = 0
    load = Mock(return_value=library)
    monkeypatch.setattr(ctypes, "CDLL", load)
    worker.disable_linux_core_dumps()
    load.assert_called_once_with(None, use_errno=True)
    library.prctl.assert_called_once_with(4, 0, 0, 0, 0)
    assert library.prctl.argtypes == (ctypes.c_int,) + (ctypes.c_ulong,) * 4
    assert library.prctl.restype is ctypes.c_int


def test_failed_linux_core_policy_is_infrastructure(monkeypatch: pytest.MonkeyPatch) -> None:
    """An unavailable policy must not silently enable slow crash-reporting paths."""
    library = Mock()
    library.prctl.return_value = -1
    monkeypatch.setattr(ctypes, "CDLL", Mock(return_value=library))
    monkeypatch.setattr(ctypes, "get_errno", lambda: errno.EPERM)
    with pytest.raises(OSError, match="PR_SET_DUMPABLE") as error:
        worker.disable_linux_core_dumps()
    assert error.value.errno == errno.EPERM
