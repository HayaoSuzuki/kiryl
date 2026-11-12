# Copyright (c) 2026 kiryl contributors
"""Native product CLI acceptance; build with cargo build before running."""

import ctypes
import json
import os
import signal
import subprocess
import sys
import time
from contextlib import suppress
from operator import itemgetter
from pathlib import Path
from typing import Any
from unittest.mock import patch

import pytest

BINARY = Path(
    os.environ.get(
        "KIRYL_BINARY",
        str(
            Path(__file__).parents[2]
            / "target"
            / "debug"
            / ("kiryl.exe" if sys.platform == "win32" else "kiryl")
        ),
    )
)

pytestmark = pytest.mark.skipif(not BINARY.exists(), reason="cargo build required")


def invoke(root: Path, *args: str) -> subprocess.CompletedProcess[bytes]:
    """Run the built product against an isolated project and selected interpreter."""
    return subprocess.run(  # noqa: S603 -- built product and owned temporary project
        [str(BINARY), *args, "--python", sys.executable],
        cwd=root,
        capture_output=True,
        check=False,
        timeout=20,
    )


def project(root: Path, body: str) -> None:
    """Write project configuration outside the repository's pytest settings."""
    (root / "pytest.ini").write_text("[pytest]\n")
    (root / "test_target.py").write_text(body)


def run(root: Path, name: str = "report", *extra: str) -> tuple[int, dict[str, Any]]:
    """Use finite slices so campaign boundaries are exercised repeatedly."""
    result = invoke(
        root,
        "run",
        "test_target.py",
        "--best-effort",
        "--duration",
        "10",
        "--slice",
        "0.5",
        "--store",
        str(root / "store"),
        "--report",
        str(root / f"{name}.json"),
        *extra,
    )
    assert result.returncode != 2, result.stderr  # noqa: PLR2004 -- infrastructure
    return result.returncode, json.loads((root / f"{name}.json").read_text())


def read_record(path: Path) -> dict[str, Any]:
    """Read one immutable finding record from the store."""
    return json.loads(path.read_bytes())


def wait_for_marker(marker: Path, process: subprocess.Popen[bytes], timeout: float) -> None:
    """Wait for worker readiness with a bounded polling deadline."""
    deadline = time.monotonic() + timeout
    while not marker.exists() and process.poll() is None and time.monotonic() < deadline:
        time.sleep(0.02)


def stop_process(process: subprocess.Popen[bytes] | None) -> None:
    """Reap a test process even when its scenario raises before signaling it."""
    if process is not None and process.poll() is None:
        process.kill()
        process.communicate(timeout=10)


def wait_for_linux_child(child_pid: int) -> Path:
    """Allow process-group cleanup to become observable before inspecting status."""
    deadline = time.monotonic() + 2
    status = Path(f"/proc/{child_pid}/status")
    while status.exists() and "State:\tZ" not in status.read_text() and time.monotonic() < deadline:
        time.sleep(0.02)
    return status


def kill_pid_file(pid_file: Path, sig: int) -> None:
    """Clean a leftover child without treating an already reaped PID as an error."""
    if pid_file.exists():
        with suppress(ProcessLookupError):
            os.kill(int(pid_file.read_text()), sig)


def assert_windows_child_exited(kernel: ctypes.CDLL, handle: int | None) -> None:
    """Interpret both valid exited handles and already reaped Windows PIDs."""
    assert sys.platform == "win32"

    if bool(handle):
        code = ctypes.c_uint32()
        assert kernel.GetExitCodeProcess(handle, ctypes.byref(code))
        assert code.value != 259  # noqa: PLR2004 -- STILL_ACTIVE
    else:
        assert ctypes.get_last_error() in (87, 1168)


def close_windows_child(kernel: ctypes.CDLL, handle: int | None, pid_file: Path) -> None:
    """Release native handles and clean a child after a failed assertion."""
    if bool(handle):
        kernel.CloseHandle(handle)
    if pid_file.exists():
        try:
            os.kill(int(pid_file.read_text()), signal.SIGTERM)
        except OSError as error:
            if getattr(error, "winerror", None) not in (87, 1168):
                raise


def allocate_console(kernel: ctypes.CDLL, *, allocated: bool) -> None:
    """Acquire a console only when the Windows runner does not already own one."""
    assert sys.platform == "win32"

    if allocated and not kernel.AllocConsole():
        raise ctypes.WinError(ctypes.get_last_error())


def free_console(kernel: ctypes.CDLL, *, allocated: bool) -> None:
    """Release only the console allocated by this test."""
    if allocated:
        kernel.FreeConsole()


def test_passing_run_restarts_persistent_corpus(tmp_path: Path) -> None:
    """Finite successful searches remain successful when resumed from their store."""
    project(
        tmp_path,
        "from hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_pass(n):\n    assert n == n\n",
    )
    code, report = run(tmp_path)
    assert code in (0, 1)
    assert report["targets"][0]["totals"]["useful"] > 0
    corpus = tmp_path / "store" / "target-0" / "corpus.json"
    assert json.loads(corpus.read_text())
    code, _ = run(tmp_path, "restart")
    assert code in (0, 1)


@pytest.fixture
def failing_original(tmp_path: Path) -> tuple[Path, bytes]:
    """Persist one always-failing property and its immutable original candidate."""
    project(
        tmp_path,
        "from hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_fail(n):\n    assert False\n",
    )
    code, _ = run(tmp_path)
    assert code == 1
    original = next((tmp_path / "store").rglob("finding-*.bin"))
    before = original.read_bytes()
    return original, before


@pytest.mark.parametrize("command", ["replay", "minimize"])
def test_failure_replay_and_minimize_preserve_original(
    tmp_path: Path, failing_original: tuple[Path, bytes], command: str
) -> None:
    """Fresh verification and minimization preserve immutable original bytes."""
    original, before = failing_original
    result = invoke(tmp_path, command, str(original), "--best-effort", "--duration", "10")
    assert result.returncode == 1, result.stderr
    assert original.read_bytes() == before


def test_replay_rejects_changed_source(
    tmp_path: Path, failing_original: tuple[Path, bytes]
) -> None:
    """Source identity drift rejects replay without changing the saved original."""
    original, before = failing_original
    source = tmp_path / "test_target.py"
    source.write_text(source.read_text() + "\n# changed source\n")
    result = invoke(tmp_path, "replay", str(original), "--best-effort", "--duration", "2")
    assert result.returncode == 2  # noqa: PLR2004 -- infrastructure exit contract
    assert original.read_bytes() == before


def test_selection_retains_parameter_fixtures_and_stateful(tmp_path: Path) -> None:
    """Selection preserves real pytest IDs while the adapter runs their fixtures."""
    project(
        tmp_path,
        """
import pytest
from hypothesis import given, settings, HealthCheck, strategies as st
from hypothesis.stateful import RuleBasedStateMachine, rule
@pytest.fixture
def value(): return 7
@pytest.mark.parametrize("tag", ["a", "b"])
@settings(suppress_health_check=[HealthCheck.function_scoped_fixture])
@given(n=st.integers())
def test_selected(value, tag, n): assert value == 7
@given(st.integers())
def test_excluded(n): assert False
class Machine(RuleBasedStateMachine):
    @rule()
    def step(self): pass
TestSelectedMachine = Machine.TestCase
""",
    )
    code, report = run(tmp_path, "selected", "-k", "selected and not excluded")
    assert code in (0, 1)
    targets = list(map(itemgetter("target"), report["targets"]))
    assert len(targets) == 3  # noqa: PLR2004 -- two variants and state machine
    assert "[a]" in "\n".join(targets)
    assert "[b]" in "\n".join(targets)
    assert "TestSelectedMachine" in "\n".join(targets)


def test_case_timeout_preserves_active_input(tmp_path: Path) -> None:
    """A killed active case retains its original input rather than a replay token."""
    project(
        tmp_path,
        "import time\nfrom hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_hang(n):\n    time.sleep(10)\n",
    )
    result = invoke(
        tmp_path,
        "run",
        "test_target.py",
        "--best-effort",
        "--duration",
        "8",
        "--case-timeout",
        "0.1",
        "--store",
        str(tmp_path / "store"),
        "--report",
        str(tmp_path / "timeout.json"),
    )
    assert result.returncode == 1, result.stderr
    records = list(map(read_record, (tmp_path / "store").rglob("finding-*.bin")))
    assert records
    assert set(map(itemgetter("test_id"), records)) == {"test_target.py::test_hang"}
    assert set(map(type, map(itemgetter("input"), records))) == {list}
    timed_out = list(
        filter(lambda record: record["observation"]["kind"] == "case_timeout", records)
    )
    assert timed_out
    assert timed_out[0]["input"] == [0] * 512


def test_short_budget_keeps_unexecuted_target_visible(tmp_path: Path) -> None:
    """Exhausted setup/case time cannot remove later selected targets from reports."""
    project(
        tmp_path,
        "import time\nfrom hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_first(n):\n    time.sleep(60)\n"
        "@given(st.integers())\ndef test_later(n):\n    pass\n",
    )
    (tmp_path / "pytest.ini").write_text("[pytest]\naddopts = --randomly-dont-reorganize\n")
    result = invoke(
        tmp_path,
        "run",
        "test_target.py",
        "--best-effort",
        "--duration",
        "10",
        "--store",
        str(tmp_path / "store"),
        "--report",
        str(tmp_path / "short.json"),
    )
    assert result.returncode in (0, 1), result.stderr
    report = json.loads((tmp_path / "short.json").read_text())
    later = next(filter(lambda entry: entry["target"].endswith("test_later"), report["targets"]))
    assert later["attempts"] == 0


@pytest.mark.skipif(sys.platform != "darwin", reason="macOS lacks native hard backend")
def test_hard_limit_rejection_precedes_target_import(tmp_path: Path) -> None:
    """Unavailable hard enforcement rejects collection before user imports run."""
    marker = tmp_path / "imported.txt"
    project(tmp_path, f"from pathlib import Path\nPath({str(marker)!r}).write_text('imported')\n")
    result = invoke(
        tmp_path, "run", "test_target.py", "--duration", "1", "--store", str(tmp_path / "store")
    )
    assert result.returncode == 2  # noqa: PLR2004 -- infrastructure exit
    assert not marker.exists()


def test_binary_fd_descendant_output_is_bounded(tmp_path: Path) -> None:
    """Binary output from both worker and child never becomes control frames."""
    project(
        tmp_path,
        """
import os, subprocess, sys
from hypothesis import given, strategies as st
@given(st.integers())
def test_output(n):
    os.write(1, b"worker-out\\xff" * 4096)
    os.write(2, b"worker-err\\xfe" * 4096)
    child = "import os;os.write(1,b'child-out'*4096);os.write(2,b'child-err'*4096)"
    subprocess.run([sys.executable, "-c", child], check=True)
""",
    )
    (tmp_path / "pytest.ini").write_text("[pytest]\naddopts = -s\n")
    code, report = run(tmp_path, "output", "--output-bytes", "1024")
    assert code in (0, 1)
    logs = report["targets"][0]["report"]["logs"]
    assert sum(map(len, map(itemgetter("tail"), logs))) <= 1024  # noqa: PLR2004 -- configured limit
    retained = report["targets"][0]["retained_output"]
    assert len(retained["tail"]) <= 1024  # noqa: PLR2004 -- campaign retention bound
    assert retained["observed"] > 1024  # noqa: PLR2004 -- configured limit
    assert retained["truncated"] is True
    assert report["targets"][0]["totals"]["useful"] > 0


def test_native_crash_original_and_fresh_raw_replay(tmp_path: Path) -> None:
    """A native abort preserves the exact candidate and reproduces in a new worker."""
    project(
        tmp_path,
        "import os\nfrom hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_abort(n):\n    os.abort()\n",
    )
    code, report = run(tmp_path, "crash")
    assert code == 1
    crash = next(
        filter(
            lambda item: item["kind"] == "native_crash",
            report["targets"][0]["report"]["outcomes"],
        )
    )
    original = Path(crash["original"])
    before = original.read_bytes()
    assert json.loads(before)["input"] == [0] * 512
    result = invoke(
        tmp_path,
        "replay",
        str(original),
        "--best-effort",
        "--duration",
        "10",
        "--report",
        str(tmp_path / "replay.json"),
    )
    assert result.returncode == 1, result.stderr
    assert original.read_bytes() == before
    replay = json.loads((tmp_path / "replay.json").read_text())
    assert replay["outcomes"][0]["kind"] == "native_crash"


@pytest.mark.skipif(
    sys.platform == "win32", reason="POSIX SIGINT; Windows console event requires runner"
)
def test_untimed_campaign_interrupt_persists_report(tmp_path: Path) -> None:
    """SIGINT ends an untimed campaign with exit 130 and an interrupted report."""
    marker = tmp_path / "started.txt"
    project(
        tmp_path,
        "import time\nfrom pathlib import Path\nfrom hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_wait(n):\n"
        f"    Path({str(marker)!r}).write_text('started')\n    time.sleep(20)\n",
    )
    process = subprocess.Popen(  # noqa: S603 -- built product and temporary project
        [
            str(BINARY),
            "run",
            "test_target.py",
            "--python",
            sys.executable,
            "--best-effort",
            "--store",
            str(tmp_path / "store"),
            "--report",
            str(tmp_path / "interrupt.json"),
        ],
        cwd=tmp_path,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    try:
        wait_for_marker(marker, process, 10)
        assert marker.exists(), process.poll()
        process.send_signal(signal.SIGINT)
        stdout, stderr = process.communicate(timeout=10)
        assert process.returncode == 130, (stdout, stderr)  # noqa: PLR2004 -- public exit
    finally:
        stop_process(process)
    assert json.loads((tmp_path / "interrupt.json").read_text())["interrupted"] is True


@pytest.mark.skipif(sys.platform != "win32", reason="direct Windows standard handle")
def test_windows_direct_standard_handle_output(tmp_path: Path) -> None:
    """Win32 WriteFile output follows the same bounded drain as CRT fd writes."""
    project(
        tmp_path,
        """
import ctypes
from hypothesis import given, strategies as st
@given(st.integers())
def test_handle(n):
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.GetStdHandle.restype = ctypes.c_void_p
    kernel.WriteFile.argtypes = (ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint32,
                                ctypes.c_void_p, ctypes.c_void_p)
    payload = ctypes.create_string_buffer(b"direct-handle" * 4096)
    written = ctypes.c_uint32()
    for handle_id in (-11, -12):
        assert kernel.WriteFile(kernel.GetStdHandle(handle_id), payload, len(payload),
                                ctypes.byref(written), None)
""",
    )
    (tmp_path / "pytest.ini").write_text("[pytest]\naddopts = -s\n")
    code, report = run(tmp_path, "handles", "--output-bytes", "1024")
    assert code in (0, 1)
    logs = report["targets"][0]["report"]["logs"]
    assert sum(map(len, map(itemgetter("tail"), logs))) <= 1024  # noqa: PLR2004
    retained = report["targets"][0]["retained_output"]
    assert len(retained["tail"]) <= 1024  # noqa: PLR2004 -- campaign retention bound
    assert retained["observed"] > 1024  # noqa: PLR2004 -- configured limit
    assert retained["truncated"] is True


@pytest.mark.skipif(sys.platform != "linux", reason="native Linux process-tree cleanup")
def test_timeout_cleans_worker_descendant_tree(tmp_path: Path) -> None:
    """A worker timeout cannot leave its descendant running outside its lifetime."""
    assert sys.platform == "linux"
    pid_file = tmp_path / "child.txt"
    project(
        tmp_path,
        "import subprocess, sys, time\nfrom pathlib import Path\n"
        "from hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_tree(n):\n"
        "    child = subprocess.Popen([sys.executable, '-c', 'import time;time.sleep(60)'])\n"
        f"    Path({str(pid_file)!r}).write_text(str(child.pid))\n"
        "    time.sleep(60)\n",
    )
    try:
        result = invoke(
            tmp_path,
            "run",
            "test_target.py",
            "--best-effort",
            "--duration",
            "2",
            "--case-timeout",
            "0.2",
            "--store",
            str(tmp_path / "store"),
            "--report",
            str(tmp_path / "tree.json"),
        )
        assert result.returncode == 1, result.stderr
        child_pid = int(pid_file.read_text())
        # Ordinary process-group cleanup is observable even when the backend
        # conservatively disclaims hard cleanup of detached descendants.
        status = wait_for_linux_child(child_pid)
        assert not status.exists() or "State:\tZ" in status.read_text()
    finally:
        kill_pid_file(pid_file, signal.SIGKILL)


@pytest.mark.skipif(sys.platform != "win32", reason="native Windows Job Object descendant cleanup")
def test_windows_timeout_cleans_descendant_job(tmp_path: Path) -> None:
    """Terminating a worker job also terminates the Python child it created."""
    assert sys.platform == "win32"
    import ctypes  # noqa: PLC0415 -- native Windows process status

    pid_file = tmp_path / "child.txt"
    project(
        tmp_path,
        "import subprocess, sys, time\nfrom pathlib import Path\n"
        "from hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_job(n):\n"
        "    child = subprocess.Popen([sys.executable, '-c', 'import time;time.sleep(60)'])\n"
        f"    Path({str(pid_file)!r}).write_text(str(child.pid))\n"
        "    time.sleep(60)\n",
    )
    handle = None
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.OpenProcess.argtypes = (ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32)
    kernel.OpenProcess.restype = ctypes.c_void_p
    kernel.GetExitCodeProcess.argtypes = (ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint32))
    kernel.CloseHandle.argtypes = (ctypes.c_void_p,)
    try:
        result = invoke(
            tmp_path,
            "run",
            "test_target.py",
            "--best-effort",
            "--duration",
            "2",
            "--case-timeout",
            "0.2",
            "--store",
            str(tmp_path / "store"),
            "--report",
            str(tmp_path / "job.json"),
        )
        assert result.returncode == 1, result.stderr
        child_pid = int(pid_file.read_text())
        report = json.loads((tmp_path / "job.json").read_text())
        assert report["targets"][0]["report"]["capabilities"]["tree_cleanup"] is True
        handle = kernel.OpenProcess(0x1000, 0, child_pid)  # PROCESS_QUERY_LIMITED_INFORMATION
        assert_windows_child_exited(kernel, handle)
    finally:
        close_windows_child(kernel, handle, pid_file)


def test_fixture_setup_error_retains_diagnostic_detail(tmp_path: Path) -> None:
    """A pytest setup failure is infrastructure with the original diagnostic."""
    project(
        tmp_path,
        """
import pytest
from hypothesis import given, strategies as st
@pytest.fixture
def broken():
    raise RuntimeError("setup sentinel diagnostic")
@given(st.integers())
def test_setup(broken, n):
    pass
""",
    )
    result = invoke(
        tmp_path,
        "run",
        "test_target.py",
        "--best-effort",
        "--duration",
        "2",
        "--store",
        str(tmp_path / "store"),
        "--report",
        str(tmp_path / "setup.json"),
    )
    assert result.returncode == 2  # noqa: PLR2004 -- infrastructure exit
    report = json.loads((tmp_path / "setup.json").read_text())
    outcomes = report["targets"][0]["report"]["outcomes"]
    infrastructure = list(filter(lambda item: item["kind"] == "infrastructure", outcomes))
    assert infrastructure
    assert "setup sentinel diagnostic" in "\n".join(map(itemgetter("detail"), infrastructure))


def test_collection_error_replaces_stale_success_report(tmp_path: Path) -> None:
    """Collection failure atomically replaces a previous successful report."""
    project(tmp_path, "raise RuntimeError('collection sentinel diagnostic')\n")
    destination = tmp_path / "report.json"
    destination.write_text('{"status":"success","stale":true}')
    result = invoke(
        tmp_path,
        "run",
        "test_target.py",
        "--best-effort",
        "--duration",
        "2",
        "--store",
        str(tmp_path / "store"),
        "--report",
        str(destination),
    )
    assert result.returncode == 2  # noqa: PLR2004 -- infrastructure exit
    report = json.loads(destination.read_text())
    assert report["status"] == "infrastructure"
    assert "stale" not in report
    assert report["infrastructure_errors"]


def test_store_lock_failure_remains_visible_in_target_report(tmp_path: Path) -> None:
    """An existing writer lock yields a target infrastructure record and detail."""
    project(
        tmp_path,
        "from hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_pass(n):\n    pass\n",
    )
    namespace = tmp_path / "store" / "target-0"
    namespace.mkdir(parents=True)
    lock = namespace / "writer.lock"
    lock.write_text("existing owner")
    result = invoke(
        tmp_path,
        "run",
        "test_target.py",
        "--best-effort",
        "--duration",
        "2",
        "--store",
        str(tmp_path / "store"),
        "--report",
        str(tmp_path / "locked.json"),
    )
    assert result.returncode == 2  # noqa: PLR2004 -- infrastructure exit
    report = json.loads((tmp_path / "locked.json").read_text())
    target = report["targets"][0]
    assert target["status"] == "infrastructure"
    assert target["report"]["outcomes"][0]["kind"] == "infrastructure"
    assert target["report"]["outcomes"][0]["detail"]
    assert lock.read_text() == "existing owner"


@pytest.mark.skipif(sys.platform != "win32", reason="native Windows console control event")
def test_windows_untimed_console_break_persists_report(tmp_path: Path) -> None:
    """A real console CTRL_BREAK_EVENT interrupts the product process group."""
    assert sys.platform == "win32"
    import ctypes  # noqa: PLC0415 -- native Windows console ownership

    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.GetConsoleCP.restype = ctypes.c_uint32
    kernel.AllocConsole.restype = ctypes.c_int
    kernel.FreeConsole.restype = ctypes.c_int
    allocated = kernel.GetConsoleCP() == 0
    allocate_console(kernel, allocated=allocated)
    marker = tmp_path / "started.txt"
    project(
        tmp_path,
        "import time\nfrom pathlib import Path\nfrom hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_wait(n):\n"
        f"    Path({str(marker)!r}).write_text('started')\n    time.sleep(20)\n",
    )
    process = None
    try:
        process = subprocess.Popen(  # noqa: S603 -- built product and temporary project
            [
                str(BINARY),
                "run",
                "test_target.py",
                "--python",
                sys.executable,
                "--best-effort",
                "--store",
                str(tmp_path / "store"),
                "--report",
                str(tmp_path / "interrupt.json"),
            ],
            cwd=tmp_path,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            creationflags=subprocess.CREATE_NEW_PROCESS_GROUP,
        )
        wait_for_marker(marker, process, 15)
        assert marker.exists(), process.poll()
        process.send_signal(signal.CTRL_BREAK_EVENT)
        stdout, stderr = process.communicate(timeout=15)
        assert process.returncode == 130, (stdout, stderr)  # noqa: PLR2004 -- public exit
        report = json.loads((tmp_path / "interrupt.json").read_text())
        assert report["interrupted"] is True
    finally:
        stop_process(process)
        free_console(kernel, allocated=allocated)


def test_campaign_store_lock_failure_retains_completed_selection(tmp_path: Path) -> None:
    """A root store lock fails after collection while retaining structured selection context."""
    project(
        tmp_path,
        "from hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_pass(n):\n    pass\n",
    )
    store = tmp_path / "store"
    store.mkdir()
    lock = store / "writer.lock"
    lock.write_text("existing campaign owner")
    destination = tmp_path / "campaign-locked.json"
    result = invoke(
        tmp_path,
        "run",
        "test_target.py",
        "--best-effort",
        "--duration",
        "2",
        "--store",
        str(store),
        "--report",
        str(destination),
    )
    assert result.returncode == 2  # noqa: PLR2004 -- infrastructure exit
    report = json.loads(destination.read_text())
    assert report["status"] == "infrastructure"
    assert report["selection_complete"] is True
    assert report["infrastructure_errors"]
    assert lock.read_text() == "existing campaign owner"


def test_minimize_rejects_raw_crash_without_changing_original(tmp_path: Path) -> None:
    """Raw crash evidence has no semantic shrink token and remains immutable on rejection."""
    project(
        tmp_path,
        "import os\nfrom hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_abort(n):\n    os.abort()\n",
    )
    code, campaign = run(tmp_path, "raw-minimize")
    assert code == 1
    crash = next(
        filter(
            lambda item: item["kind"] == "native_crash",
            campaign["targets"][0]["report"]["outcomes"],
        )
    )
    original = Path(crash["original"])
    before = original.read_bytes()
    destination = tmp_path / "raw-minimize-error.json"
    result = invoke(
        tmp_path,
        "minimize",
        str(original),
        "--best-effort",
        "--duration",
        "2",
        "--report",
        str(destination),
    )
    assert result.returncode == 2  # noqa: PLR2004 -- infrastructure exit
    report = json.loads(destination.read_text())
    assert report["status"] == "infrastructure"
    assert report["selection_complete"] is True
    assert "semantic minimization unavailable" in "\n".join(report["infrastructure_errors"])
    assert original.read_bytes() == before


@pytest.fixture
def reordered_campaign(tmp_path: Path) -> tuple[dict[str, Any], subprocess.CompletedProcess[bytes]]:
    """Exercise two phases of one resume lifecycle with unchanged source/config."""
    project(
        tmp_path,
        "from hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_alpha(n): pass\n"
        "@given(st.integers())\ndef test_zeta(n): pass\n",
    )
    (tmp_path / "conftest.py").write_text(
        "import os\nimport pytest\n"
        "@pytest.hookimpl(trylast=True)\n"
        "def pytest_collection_modifyitems(items):\n"
        "    items.sort(key=lambda item: item.nodeid, "
        "reverse=os.environ.get('KIRYL_TEST_ORDER') == 'reverse')\n"
    )
    with patch.dict(os.environ, {"KIRYL_TEST_ORDER": "forward"}):
        first = invoke(
            tmp_path,
            "run",
            "test_target.py",
            "--best-effort",
            "--duration",
            "12",
            "--store",
            str(tmp_path / "store"),
            "--report",
            str(tmp_path / "first.json"),
        )
    assert first.returncode in (0, 1), first.stderr
    report = json.loads((tmp_path / "first.json").read_text())
    with patch.dict(os.environ, {"KIRYL_TEST_ORDER": "reverse"}):
        second = invoke(
            tmp_path,
            "run",
            "test_target.py",
            "--best-effort",
            "--duration",
            "12",
            "--store",
            str(tmp_path / "store"),
            "--report",
            str(tmp_path / "second.json"),
        )
    return report, second


def test_resume_accepts_reordered_collection_with_unchanged_identity(
    tmp_path: Path, reordered_campaign: tuple[dict[str, Any], subprocess.CompletedProcess[bytes]]
) -> None:
    """Plugin ordering must not bind an existing namespace to a different node."""
    first, result = reordered_campaign
    assert result.returncode in (0, 1), result.stderr
    resumed = json.loads((tmp_path / "second.json").read_text())
    assert first["environment_identity"] == resumed["environment_identity"]
    assert set(map(itemgetter("status"), first["targets"])) == {"searched"}
    assert set(map(itemgetter("status"), resumed["targets"])) == {"searched"}
    assert list(map(itemgetter("target"), resumed["targets"])) == [
        "test_target.py::test_alpha",
        "test_target.py::test_zeta",
    ]
    assert min(map(itemgetter("attempts"), resumed["targets"])) > 0
    assert len(list((tmp_path / "store").glob("target-*/corpus.json"))) == len(resumed["targets"])
