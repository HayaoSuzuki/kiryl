# Copyright (c) 2026 kiryl contributors
"""Real capability evidence; unavailable native hard enforcement stays unverified."""

import json
import os
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import TYPE_CHECKING, cast
from unittest.mock import patch

import pytest
from test_controller import BINARY
from test_controller import probe_run as probe_run  # noqa: PLC0414 -- shared pytest fixture
from test_controller import process_project as process_project  # noqa: PLC0414 -- shared pytest fixture

from model import ResourceBackend

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
def test_allocate_within_limit(value):
    child = subprocess.Popen([sys.executable, "-c", (
        "import time; data=bytearray(64*1024*1024); time.sleep(1)"
    )])
    assert child.wait() == 0

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
    capabilities = cast("dict[str, object]", cast("dict[str, object]", report)["capabilities"])
    return {**capabilities, "backend": ResourceBackend(capabilities["backend"])}


@pytest.fixture
def hard_capabilities(probe_run: Callable[..., tuple[int, Report]]) -> dict[str, object]:
    """Require a real hard backend before executing a hard-enforcement scenario."""
    _, report = probe_run(("ok",))
    capabilities = _capabilities(report)
    if not capabilities["tree_memory_hard"]:
        pytest.skip("native delegated cgroup/Job Object unavailable; hard gate remains unverified")
    return capabilities


@pytest.mark.usefixtures("hard_capabilities")
def test_hard_cap_allows_attached_target(
    probe_run: Callable[..., tuple[int, Report]], process_project: Path
) -> None:
    """A native capable host admits the target with hard enforcement required."""
    code, _ = probe_run(("marker",), allow_best_effort=False)
    assert code == 0
    assert (process_project / "target-started.txt").exists()


@pytest.mark.usefixtures("portable_capabilities")
def test_unavailable_hard_cap_refuses_before_target(
    probe_run: Callable[..., tuple[int, Report]], process_project: Path
) -> None:
    """Default refusal happens before collection or target execution on fallback hosts."""
    code, report = probe_run(("marker",), allow_best_effort=False)
    assert code == INFRASTRUCTURE_EXIT
    assert not (process_project / "target-started.txt").exists()
    assert report["outcomes"][0]["kind"] == "infrastructure"


@pytest.fixture
def portable_capabilities(probe_run: Callable[..., tuple[int, Report]]) -> dict[str, object]:
    """Observe the portable contract only on a host selecting that backend."""
    code, report = probe_run(("ok",))
    assert code == 0
    capabilities = _capabilities(report)
    if capabilities["backend"] != ResourceBackend.PORTABLE:
        pytest.skip("native hard backend selected; portable contract needs a fallback host")
    return capabilities


def test_fallback_is_explicit_about_containment(portable_capabilities: dict[str, object]) -> None:
    """A portable process group is weaker than whole-tree containment."""
    assert portable_capabilities["tree_memory_hard"] is False
    assert portable_capabilities["tree_cleanup"] is False
    assert portable_capabilities["reason"]


def test_startup_ignores_unattached_pythonpath_hooks(
    probe_run: Callable[..., tuple[int, Report]],
    process_project: Path,
) -> None:
    """Trusted isolated bootstrap cannot run PYTHONPATH startup code before attachment."""
    hook = process_project / "early-startup.txt"
    (process_project / "sitecustomize.py").write_text(
        f"from pathlib import Path\nPath({str(hook)!r}).write_text('too early')\n"
    )
    with patch.dict(os.environ, {"PYTHONPATH": str(process_project)}):
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


@pytest.fixture
def aggregate_memory_run(
    probe_run: Callable[..., tuple[int, Report]], hard_capabilities: dict[str, object]
) -> tuple[int, Report]:
    """Execute one aggregate allocation after actual capability discovery."""
    assert hard_capabilities["tree_memory_hard"] is True
    return probe_run(("allocate",), memory_bytes=MEMORY_LIMIT, case_ms=3000)


def test_combined_memory_requires_backend_evidence(
    aggregate_memory_run: tuple[int, Report],
) -> None:
    """Only a real capable environment can establish aggregate hard enforcement."""
    code, report = aggregate_memory_run
    assert code == 1
    finding = report["outcomes"][0]
    assert finding["kind"] == "oom"
    assert json.loads(Path(finding["original"]).read_text())["input"] == [8]


@pytest.mark.skipif(os.name == "nt", reason="POSIX child liveness assertion")
def test_aggregate_memory_kill_removes_child(
    aggregate_memory_run: tuple[int, Report], process_project: Path
) -> None:
    """The aggregate limit kills the descendant allocating beyond its allowance."""
    code, _ = aggregate_memory_run
    assert code == 1
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


@pytest.mark.usefixtures("hard_capabilities")
def test_setup_memory_limit_is_infrastructure(
    probe_run: Callable[..., tuple[int, Report]], process_project: Path
) -> None:
    """A resource failure before any dispatched example cannot implicate input bytes."""
    code, report = probe_run(("marker",), memory_bytes=16 * 1024 * 1024)
    assert code == INFRASTRUCTURE_EXIT
    assert not (process_project / "target-started.txt").exists()
    assert report["outcomes"][0]["original"] is None


@pytest.mark.usefixtures("hard_capabilities")
def test_two_trees_have_independent_allowances(process_project: Path) -> None:
    """Two independent controllers may each consume their own hard allowance."""

    def launch(index: int) -> tuple[int, dict[str, object]]:
        out = process_project / f"independent-{index}"
        config = process_project / f"independent-{index}.json"
        config.write_text(
            json.dumps(
                {
                    "python": sys.executable,
                    "pytest_args": [str(process_project), "-q", "-s"],
                    "invocations": [
                        {"test_id": "test_process.py::test_allocate_within_limit", "inputs": [[8]]}
                    ],
                    "limits": {
                        "campaign_ms": 5000,
                        "case_ms": 5000,
                        "tree_memory_bytes": 160 * 1024 * 1024,
                    },
                }
            )
        )
        result = subprocess.run(  # noqa: S603 -- independent owned probe/config
            [str(BINARY), "run", "--config", str(config), "--out", str(out)],
            capture_output=True,
            timeout=10,
            check=False,
        )
        return result.returncode, cast(
            "dict[str, object]", json.loads((out / "report.json").read_text())
        )

    with ThreadPoolExecutor(max_workers=2) as pool:
        results = list(pool.map(launch, range(2)))
    assert all(code == 0 for code, _ in results)
