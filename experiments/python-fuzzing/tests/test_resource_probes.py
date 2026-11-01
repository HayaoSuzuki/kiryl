# Copyright (c) 2026 kiryl contributors
"""Real capability evidence; unavailable native hard enforcement stays unverified."""

import json
import os
import time
from pathlib import Path
from typing import TYPE_CHECKING, cast

import pytest
from test_controller import probe_run as probe_run  # noqa: PLC0414 -- shared pytest fixture
from test_controller import process_project as process_project  # noqa: PLC0414 -- shared pytest fixture

if TYPE_CHECKING:
    from collections.abc import Callable

    from test_controller import Report

INFRASTRUCTURE_EXIT = 2
MEMORY_LIMIT = 96 * 1024 * 1024


@pytest.fixture(autouse=True)
def resource_targets(process_project: Path) -> None:
    """Add independently authored allocator and surviving-child targets."""
    with (process_project / "test_process.py").open("a") as source:
        source.write("""
import subprocess
from pathlib import Path
import sys

@given(st.integers(0, 255))
def test_marker(value):
    Path("target-started.txt").write_text("started")

@given(st.integers(0, 255))
def test_child(value):
    child = subprocess.Popen([sys.executable, "-c", (
        "import time; from pathlib import Path; time.sleep(0.8); "
        "Path('child-finished.txt').write_text('escaped')"
    )])
    Path("child-pid.txt").write_text(str(child.pid))

@given(st.integers(0, 255))
def test_flood_hang(value):
    for _ in range(32):
        os.write(1, b"x" * 4096)
        os.write(2, b"y" * 4096)
    while True:
        time.sleep(0.01)

@given(st.integers(0, 255))
def test_unrelated_exit(value):
    os._exit(7)

@given(st.integers(0, 255))
def test_allocate(value):
    child = subprocess.Popen([sys.executable, "-c", (
        "import time; data=bytearray(80*1024*1024); time.sleep(10)"
    )])
    Path("child-pid.txt").write_text(str(child.pid))
    while True:
        time.sleep(0.01)
""")


def _capabilities(report: Report) -> dict[str, object]:
    return cast("dict[str, object]", cast("dict[str, object]", report)["capabilities"])


def test_unavailable_hard_cap_refuses_before_target(
    probe_run: Callable[..., tuple[int, Report]], process_project: Path
) -> None:
    """Default refusal must happen before collection or any target execution."""
    code, report = probe_run(("marker",), allow_best_effort=False)
    if _capabilities(report)["tree_memory_hard"]:
        assert code == 0
        assert (process_project / "target-started.txt").exists()
    else:
        assert code == INFRASTRUCTURE_EXIT
        assert not (process_project / "target-started.txt").exists()
        assert report["outcomes"][0]["kind"] == "infrastructure"


def test_fallback_is_explicit_about_containment(
    probe_run: Callable[..., tuple[int, Report]],
) -> None:
    """A portable process group is weaker than whole-tree containment."""
    code, report = probe_run(("ok",))
    assert code == 0
    caps = _capabilities(report)
    if caps["backend"] == "portable":
        assert caps["tree_memory_hard"] is False
        assert caps["tree_cleanup"] is False
        assert caps["reason"]


def test_startup_ignores_unattached_pythonpath_hooks(
    probe_run: Callable[..., tuple[int, Report]],
    process_project: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Trusted isolated bootstrap cannot run PYTHONPATH startup code before attachment."""
    hook = process_project / "early-startup.txt"
    (process_project / "sitecustomize.py").write_text(
        f"from pathlib import Path\nPath({str(hook)!r}).write_text('too early')\n"
    )
    monkeypatch.setenv("PYTHONPATH", str(process_project))
    code, _ = probe_run(("marker",))
    assert code == 0
    assert not hook.exists()
    assert (process_project / "target-started.txt").exists()


def test_child_is_cleaned_when_root_finishes(
    probe_run: Callable[..., tuple[int, Report]], process_project: Path
) -> None:
    """An ordinary inheriting descendant cannot outlive the owned worker group/job."""
    code, _ = probe_run(("child",))
    assert code == 0
    time.sleep(0.9)
    assert not (process_project / "child-finished.txt").exists()


def test_combined_memory_requires_backend_evidence(
    probe_run: Callable[..., tuple[int, Report]], process_project: Path
) -> None:
    """Only a real capable environment can establish aggregate hard enforcement."""
    _, capabilities = probe_run(("ok",))
    if not _capabilities(capabilities)["tree_memory_hard"]:
        pytest.skip("native delegated cgroup/Job Object unavailable; hard gate remains unverified")
    code, report = probe_run(("allocate",), memory_bytes=MEMORY_LIMIT, case_ms=3000)
    assert code == 1
    finding = report["outcomes"][0]
    assert finding["kind"] == "oom"
    assert json.loads(Path(finding["original"]).read_text())["input"] == [8]
    if os.name != "nt":
        pid = int((process_project / "child-pid.txt").read_text())
        with pytest.raises(ProcessLookupError):
            os.kill(pid, 0)


def test_unrelated_exit_is_not_oom(probe_run: Callable[..., tuple[int, Report]]) -> None:
    """No memory-event provenance means no OOM label, even for abrupt target death."""
    code, report = probe_run(("unrelated_exit",))
    assert code == INFRASTRUCTURE_EXIT
    assert report["outcomes"][0]["kind"] != "oom"
    assert json.loads(Path(report["outcomes"][0]["original"]).read_text())["input"] == [8]


def test_flood_remains_bounded_when_killed(probe_run: Callable[..., tuple[int, Report]]) -> None:
    """Termination still drains a combined, bounded tail and preserves the input."""
    code, report = probe_run(("flood_hang",), case_ms=100)
    assert code == 1
    finding = report["outcomes"][0]
    assert finding["kind"] == "case_timeout"
    assert json.loads(Path(finding["original"]).read_text())["input"] == [8]
    assert all(len(log["tail"]) <= 1024 for log in report["logs"])  # noqa: PLR2004 -- configured byte cap
    assert all(log["truncated"] for log in report["logs"])


def test_setup_memory_limit_is_infrastructure(
    probe_run: Callable[..., tuple[int, Report]], process_project: Path
) -> None:
    """A resource failure before any dispatched example cannot implicate input bytes."""
    _, report = probe_run(("ok",))
    if not _capabilities(report)["tree_memory_hard"]:
        pytest.skip("native hard backend unavailable; setup-limit gate remains unverified")
    code, report = probe_run(("marker",), memory_bytes=16 * 1024 * 1024)
    assert code == INFRASTRUCTURE_EXIT
    assert not (process_project / "target-started.txt").exists()
    assert report["outcomes"][0]["original"] is None
