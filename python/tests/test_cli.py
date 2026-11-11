# Copyright (c) 2026 kiryl contributors
"""Native product CLI acceptance; build with cargo build before running."""

import json
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest

BINARY = (
    Path(__file__).parents[2]
    / "target"
    / "debug"
    / ("kiryl.exe" if sys.platform == "win32" else "kiryl")
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
        "2",
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


def test_passing_run_restarts_persistent_corpus(tmp_path: Path) -> None:
    """Finite successful searches remain successful when resumed from their store."""
    project(
        tmp_path,
        "from hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_pass(n):\n    assert n == n\n",
    )
    code, report = run(tmp_path)
    assert code in (0, 1)
    assert report["targets"][0]["report"]["useful"] > 0
    corpus = tmp_path / "store" / "target-0" / "corpus.json"
    assert json.loads(corpus.read_text())
    code, _ = run(tmp_path, "restart")
    assert code in (0, 1)


def test_failure_original_replay_minimize_and_mismatch(tmp_path: Path) -> None:
    """Fresh verification and minimization preserve immutable original bytes."""
    project(
        tmp_path,
        "from hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_fail(n):\n    assert False\n",
    )
    code, _ = run(tmp_path)
    assert code == 1
    original = next((tmp_path / "store").rglob("finding-*.bin"))
    before = original.read_bytes()
    for command in ("replay", "minimize"):
        result = invoke(tmp_path, command, str(original), "--best-effort", "--duration", "2")
        assert result.returncode == 1, result.stderr
        assert original.read_bytes() == before
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
    targets = [entry["target"] for entry in report["targets"]]
    assert len(targets) == 3  # noqa: PLR2004 -- two variants and state machine
    assert any("[a]" in target for target in targets)
    assert any("[b]" in target for target in targets)
    assert any("TestSelectedMachine" in target for target in targets)


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
        "2",
        "--case-timeout",
        "0.1",
        "--store",
        str(tmp_path / "store"),
        "--report",
        str(tmp_path / "timeout.json"),
    )
    assert result.returncode == 1, result.stderr
    records = [
        json.loads(path.read_bytes()) for path in (tmp_path / "store").rglob("finding-*.bin")
    ]
    assert records
    assert all(record["test_id"] == "test_target.py::test_hang" for record in records)
    assert all(isinstance(record["input"], list) for record in records)
    timed_out = [record for record in records if record["observation"]["kind"] == "case_timeout"]
    assert timed_out
    assert timed_out[0]["input"] == [0] * 512


def test_short_budget_keeps_unexecuted_target_visible(tmp_path: Path) -> None:
    """Exhausted setup/case time cannot remove later selected targets from reports."""
    project(
        tmp_path,
        "import time\nfrom hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_first(n):\n    time.sleep(10)\n"
        "@given(st.integers())\ndef test_later(n):\n    pass\n",
    )
    result = invoke(
        tmp_path,
        "run",
        "test_target.py",
        "--best-effort",
        "--duration",
        "0.7",
        "--store",
        str(tmp_path / "store"),
        "--report",
        str(tmp_path / "short.json"),
    )
    assert result.returncode in (0, 1), result.stderr
    report = json.loads((tmp_path / "short.json").read_text())
    later = next(entry for entry in report["targets"] if entry["target"].endswith("test_later"))
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
    assert sum(len(log["tail"]) for log in logs) <= 1024  # noqa: PLR2004 -- configured limit
    assert sum(log["observed"] for log in logs) > 1024  # noqa: PLR2004
    assert any(log["truncated"] for log in logs)
    assert report["targets"][0]["report"]["useful"] > 0


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
        item
        for item in report["targets"][0]["report"]["outcomes"]
        if item["kind"] == "native_crash"
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
        "2",
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
    import signal  # noqa: PLC0415 -- POSIX subprocess test
    import time  # noqa: PLC0415

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
        deadline = time.monotonic() + 10
        while not marker.exists() and process.poll() is None and time.monotonic() < deadline:
            time.sleep(0.02)
        assert marker.exists(), process.poll()
        process.send_signal(signal.SIGINT)
        stdout, stderr = process.communicate(timeout=10)
        assert process.returncode == 130, (stdout, stderr)  # noqa: PLR2004 -- public exit
    finally:
        if process.poll() is None:
            process.kill()
            process.communicate(timeout=10)
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
    assert sum(len(log["tail"]) for log in logs) <= 1024  # noqa: PLR2004
    assert any(log["truncated"] for log in logs)
