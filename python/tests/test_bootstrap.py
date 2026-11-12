# Copyright (c) 2026 kiryl contributors
"""Check Linux per-worker crash reporting without changing host-wide policy."""

import ctypes
import errno
from types import SimpleNamespace
from typing import BinaryIO, Protocol, cast
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


def test_isolated_worker_ready_before_site_import() -> None:
    """The product bootstrap works with both isolated and no-site flags."""
    import subprocess  # noqa: PLC0415 -- subprocess-specific test
    import sys  # noqa: PLC0415
    from pathlib import Path  # noqa: PLC0415

    from wire import receive  # noqa: PLC0415

    root = Path(worker.__file__).parent
    script = f"import sys;sys.path.insert(0,{str(root)!r});import worker;worker.main()"
    process = subprocess.Popen(  # noqa: S603 -- owned interpreter/bootstrap
        [sys.executable, "-I", "-S", "-c", script],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    try:
        assert process.stdout is not None
        message = receive(cast("BinaryIO", process.stdout))
        assert message["kind"] == "ready"
        assert cast("dict[str, object]", message["environment"])["python"] == list(
            sys.version_info[:2]
        )
    finally:
        process.kill()
        process.communicate(timeout=10)


def test_driver_retains_stdin_before_pytest_capture() -> None:
    """Control input survives pytest replacing both sys.stdin and fd zero."""
    import subprocess  # noqa: PLC0415 -- subprocess-specific test
    import sys  # noqa: PLC0415
    from pathlib import Path  # noqa: PLC0415

    script = """
import io, os, sys
import worker
from wire import send
from adapter import CaseResult
from model import ResultKind
original = os.dup(0)
driver = worker.Driver(io.BytesIO())
with open(os.devnull, "rb") as null:
    os.dup2(null.fileno(), 0)
sys.stdin = io.StringIO("captured stdin")
assert driver.next_case("test_target") == bytes([7])
driver.observe(CaseResult("test_target", ResultKind.PASS))
os.close(original)
"""
    import io  # noqa: PLC0415 -- encode owned protocol input

    from wire import send  # noqa: PLC0415

    incoming = io.BytesIO()
    send(incoming, {"kind": "case", "case_id": 1, "input": [7]})
    send(incoming, {"kind": "ack"})
    result = subprocess.run(  # noqa: S603 -- owned bootstrap and protocol
        [sys.executable, "-c", script],
        cwd=Path(worker.__file__).parent,
        input=incoming.getvalue(),
        capture_output=True,
        check=False,
        timeout=10,
    )
    assert result.returncode == 0, result.stderr
