# Copyright (c) 2026 kiryl contributors
"""Deterministic error-path checks for native CLI acceptance-test cleanup."""

import ctypes
import signal
import sys
from contextlib import nullcontext
from pathlib import Path  # noqa: TC003 -- pytest introspects fixture annotations
from unittest.mock import Mock

import pytest
import test_cli


class WindowsCleanupError(OSError):
    """Represent a native Windows cleanup error on every test platform."""

    winerror: int


@pytest.mark.parametrize(("running", "kills"), [(True, 1), (False, 0)])
def test_process_cleanup_reaps_only_running_child(kills: int, *, running: bool) -> None:
    """Cleanup kills live children and leaves already reaped processes alone."""
    process = Mock()
    results = {True: None, False: 0}
    process.poll.return_value = results[running]
    test_cli.stop_process(process)
    assert process.kill.call_count == kills
    assert process.communicate.call_count == kills


def test_process_cleanup_accepts_missing_child() -> None:
    """Failure before process creation still permits unconditional cleanup."""
    test_cli.stop_process(None)


@pytest.mark.parametrize(
    ("markers", "returncode", "times", "sleeps"),
    [
        ([True], None, [0], 0),
        ([False], 0, [0], 0),
        ([False], None, [0, 2], 0),
        ([False, True], None, [0, 0], 1),
    ],
)
def test_marker_wait_stops_at_each_boundary(
    monkeypatch: pytest.MonkeyPatch,
    markers: list[bool],
    returncode: int | None,
    times: list[int],
    sleeps: int,
) -> None:
    """Readiness, process exit and deadline each bound readiness polling."""
    marker = Mock()
    marker.exists.side_effect = markers
    process = Mock()
    process.poll.return_value = returncode
    monkeypatch.setattr(test_cli.time, "monotonic", Mock(side_effect=times))
    sleep = Mock()
    monkeypatch.setattr(test_cli.time, "sleep", sleep)
    test_cli.wait_for_marker(marker, process, 1)
    assert sleep.call_count == sleeps


@pytest.mark.parametrize(
    ("present", "failure"), [(False, None), (True, None), (True, ProcessLookupError())]
)
def test_pid_cleanup_tolerates_reaped_process(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    failure: BaseException | None,
    *,
    present: bool,
) -> None:
    """A missing PID file or already gone process does not mask the original test failure."""
    pid_file = tmp_path / "pid"
    paths = {True: pid_file, False: tmp_path / "absent"}
    pid_file.write_text("123")
    kill = Mock(side_effect=failure)
    monkeypatch.setattr(test_cli.os, "kill", kill)
    test_cli.kill_pid_file(paths[present], signal.SIGTERM)
    assert kill.call_count == int(present)


@pytest.mark.parametrize(
    ("exists", "status", "times", "sleeps"),
    [
        ([False], "", [0], 0),
        ([True], "State:\tZ", [0], 0),
        ([True], "State:\tS", [0, 3], 0),
        ([True, False], "State:\tS", [0, 0], 1),
    ],
)
def test_linux_cleanup_wait_is_bounded(
    monkeypatch: pytest.MonkeyPatch,
    exists: list[bool],
    status: str,
    times: list[int],
    sleeps: int,
) -> None:
    """Process disappearance, zombie state and timeout each stop native status polling."""
    path = Mock()
    path.exists.side_effect = exists
    path.read_text.return_value = status
    monkeypatch.setattr(test_cli, "Path", Mock(return_value=path))
    monkeypatch.setattr(test_cli.time, "monotonic", Mock(side_effect=times))
    sleep = Mock()
    monkeypatch.setattr(test_cli.time, "sleep", sleep)
    assert test_cli.wait_for_linux_child(123) is path
    assert sleep.call_count == sleeps


@pytest.mark.parametrize("handle", [None, 1])
def test_windows_status_accepts_exited_or_reaped_child(
    monkeypatch: pytest.MonkeyPatch,
    handle: int | None,
) -> None:
    """Windows reports either an exited handle or a PID already absent from the process table."""
    monkeypatch.setattr(sys, "platform", "win32")
    monkeypatch.setattr(ctypes, "get_last_error", Mock(return_value=87), raising=False)
    test_cli.assert_windows_child_exited(Mock(), handle)


@pytest.mark.parametrize(
    "case",
    [
        (None, False, None, False),
        (1, False, None, False),
        (1, True, None, False),
        (1, True, 87, False),
        (1, True, 5, True),
    ],
)
def test_windows_cleanup_preserves_unexpected_native_errors(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    case: tuple[int | None, bool, int | None, bool],
) -> None:
    """Suppress only already reaped process errors while always closing owned handles."""
    handle, present, error_code, raises = case
    pid_file = tmp_path / "pid"
    pid_file.write_text("123")
    paths = {True: pid_file, False: tmp_path / "absent"}
    error = WindowsCleanupError("native cleanup")
    error.winerror = {None: 0, 87: 87, 5: 5}[error_code]
    failures = {None: None, 87: error, 5: error}
    monkeypatch.setattr(test_cli.os, "kill", Mock(side_effect=failures[error_code]))
    kernel = Mock()
    outcomes = {False: nullcontext(), True: pytest.raises(OSError, match="native cleanup")}
    with outcomes[raises]:
        test_cli.close_windows_child(kernel, handle, paths[present])
    assert kernel.CloseHandle.call_count == int(bool(handle))


@pytest.mark.parametrize(
    ("allocated", "success", "raises"), [(False, 0, False), (True, 1, False), (True, 0, True)]
)
def test_console_allocation_reports_native_failure(
    monkeypatch: pytest.MonkeyPatch,
    success: int,
    *,
    allocated: bool,
    raises: bool,
) -> None:
    """Allocate only a missing console and surface native allocation failure."""
    monkeypatch.setattr(sys, "platform", "win32")
    monkeypatch.setattr(ctypes, "get_last_error", Mock(return_value=5), raising=False)
    monkeypatch.setattr(ctypes, "WinError", Mock(return_value=OSError("console")), raising=False)
    kernel = Mock()
    kernel.AllocConsole.return_value = success
    outcomes = {False: nullcontext(), True: pytest.raises(OSError, match="console")}
    with outcomes[raises]:
        test_cli.allocate_console(kernel, allocated=allocated)
    assert kernel.AllocConsole.call_count == int(allocated)


@pytest.mark.parametrize("allocated", [True, False])
def test_console_cleanup_releases_only_owned_console(*, allocated: bool) -> None:
    """Never release a console that belonged to the native runner before the test."""
    kernel = Mock()
    test_cli.free_console(kernel, allocated=allocated)
    assert kernel.FreeConsole.call_count == int(allocated)
