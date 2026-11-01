# Copyright (c) 2026 kiryl contributors
"""Exercise the real Rust parent against independently authored Python targets."""

import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path
from typing import TYPE_CHECKING, TypedDict, cast

import pytest

if TYPE_CHECKING:
    from collections.abc import Callable

LOG_LIMIT = 1024
INTERRUPTED_EXIT = 130
TWO_CASES = 2


class Outcome(TypedDict):
    """Fields asserted at the real process boundary."""

    kind: str
    original: str
    verification: str
    derived: list[str]


class Tail(TypedDict):
    """Bounded raw log evidence."""

    tail: list[int]
    observed: int
    truncated: bool


class Report(TypedDict):
    """Required report fields for integration assertions."""

    outcomes: list[Outcome]
    logs: list[Tail]
    attempts: int
    interrupted: bool


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug" / ("fuzz-probe.exe" if os.name == "nt" else "fuzz-probe")


@pytest.fixture
def process_project(tmp_path: Path) -> Path:
    """Create isolated crash, hang, flood and finite-overrun properties."""
    (tmp_path / "pytest.ini").write_text("[pytest]\n")
    (tmp_path / "test_process.py").write_text(
        """import os
import time
import pytest
from hypothesis import given, strategies as st, settings, HealthCheck

@pytest.fixture
def lifecycle():
    yield
    open("teardown.txt", "a").write("done\\n")

@pytest.fixture
def broken_setup():
    raise RuntimeError("synthetic fixture failure")

@settings(suppress_health_check=[HealthCheck.function_scoped_fixture])
@given(st.integers(0, 255))
def test_setup_error(broken_setup, value):
    os.abort()

@given(st.integers(0, 255))
def test_close_control(value):
    os.close(3)
    time.sleep(2)

@given(st.integers(0, 255))
def test_wrong_id(value):
    import json
    import struct
    encoded = json.dumps({"version": 1, "kind": "result", "case_id": 999,
        "observation": {"nodeid": "test_process.py::test_wrong_id", "kind": "pass"}}).encode()
    os.write(3, struct.pack(">I", len(encoded)) + encoded)

@given(st.integers(0, 255))
def test_corrupt(value):
    # Deliberate trusted-bootstrap protocol fault, not a sandbox escape test.
    os.write(3, bytes([255]) * 4)

@given(st.integers(0, 255))
def test_crash(value):
    os.abort()

@given(st.integers(0, 255))
def test_hang(value):
    while True:
        time.sleep(0.01)

@given(st.integers(0, 255))
def test_pytest_failure(value):
    pytest.fail("synthetic oracle failure")

@given(st.integers(0, 255))
def test_failure(value):
    assert value < 1

@given(st.integers(0, 255))
def test_shrink_crash(value):
    if value == 1:
        os.abort()
    assert value < 2

@pytest.fixture
def slow_teardown():
    yield
    time.sleep(5)

@settings(suppress_health_check=[HealthCheck.function_scoped_fixture])
@given(st.integers(0, 255))
def test_setup_wait(slow_teardown, value):
    assert False

@given(st.integers(0, 255))
def test_flood(value):
    for _ in range(64):
        os.write(1, b"x" * 4096 + b"\\xff")
        os.write(2, b"y" * 4096 + b"\\xfe")

@settings(suppress_health_check=[HealthCheck.function_scoped_fixture])
@given(st.integers(0, 255))
def test_slow(lifecycle, value):
    time.sleep(0.12)

@given(st.integers(0, 255))
def test_ok(value):
    pass
"""
    )
    return tmp_path


@pytest.fixture
def probe_run(process_project: Path) -> Callable[..., tuple[int, Report]]:
    """Run a bounded real parent with a separate output namespace each time."""
    serial = 0

    def run(  # noqa: PLR0913 -- explicit independent process-probe limit overrides
        names: tuple[str, ...],
        *,
        campaign_ms: int | None = 5000,
        case_ms: int = 1000,
        slice_ms: int = 1000,
        inputs: tuple[bytes, ...] = (b"\x08",),
        allow_best_effort: bool = True,
        memory_bytes: int = 1 << 30,
    ) -> tuple[int, Report]:
        nonlocal serial
        serial += 1
        out = process_project / f"out-{serial}"
        config = {
            "python": sys.executable,
            "pytest_args": [str(process_project), "-q", "-s"],
            "invocations": [
                {"test_id": f"test_process.py::test_{name}", "inputs": [list(x) for x in inputs]}
                for name in names
            ],
            "limits": {
                "campaign_ms": campaign_ms,
                "case_ms": case_ms,
                "slice_ms": slice_ms,
                "retained_output_bytes": 1024,
                "allow_best_effort": allow_best_effort,
                "tree_memory_bytes": memory_bytes,
            },
            "seed": 0,
        }
        path = process_project / f"config-{serial}.json"
        path.write_text(json.dumps(config))
        completed = subprocess.run(  # noqa: S603 -- owned executable and synthetic local targets
            [str(BINARY), "run", "--config", str(path), "--out", str(out)],
            cwd=process_project,
            capture_output=True,
            timeout=15,
            check=False,
        )
        assert completed.returncode in (0, 1, 2, 130), completed.stderr
        return completed.returncode, cast("Report", json.loads((out / "report.json").read_text()))

    return run


def test_crash_retains_exact_original_and_continues(
    probe_run: Callable[..., tuple[int, Report]],
) -> None:
    """An aborted worker cannot take its active buffer or the next test with it."""
    code, report = probe_run(("crash", "ok"), inputs=(b"\x08\xff",))
    assert code == 1
    crash = report["outcomes"][0]
    assert crash["kind"] == "native_crash"
    assert crash["verification"] == "confirmed"
    original = json.loads(Path(crash["original"]).read_text())
    assert original["input"] == [8, 255]
    assert report["outcomes"][1]["kind"] == "pass"


def test_output_is_bounded_and_control_survives(
    probe_run: Callable[..., tuple[int, Report]],
) -> None:
    """FD-level binary output cannot corrupt control frames or grow retained logs."""
    code, report = probe_run(("flood",))
    assert code == 0
    assert len(report["logs"][0]["tail"]) <= LOG_LIMIT
    assert report["logs"][0]["observed"] > LOG_LIMIT
    assert report["logs"][0]["truncated"] is True
    assert report["outcomes"][0]["kind"] == "pass"


def test_case_timeout_is_distinct_from_campaign(
    probe_run: Callable[..., tuple[int, Report]],
) -> None:
    """Whole-campaign cancellation takes priority over a simultaneous case expiry."""
    code, report = probe_run(("hang",), case_ms=60)
    assert code == 1
    assert report["outcomes"][0]["kind"] == "case_timeout"
    code, report = probe_run(("hang",), campaign_ms=500, case_ms=5000)
    assert code == 1
    assert report["outcomes"][-1]["kind"] == "campaign_cancelled"


def test_slice_is_cooperative_and_tears_down(
    probe_run: Callable[..., tuple[int, Report]], process_project: Path
) -> None:
    """Slice expiry finishes the active case and performs pytest teardown."""
    code, report = probe_run(("slow",), slice_ms=40, case_ms=1000, inputs=(b"\x08", b"\x09"))
    assert code == 0
    assert report["attempts"] == TWO_CASES
    assert (process_project / "teardown.txt").read_text().splitlines() == ["done", "done"]


def test_failure_saved_before_fresh_verification(
    probe_run: Callable[..., tuple[int, Report]],
) -> None:
    """Fresh replay and shrinking append derived records without overwriting raw input."""
    code, report = probe_run(("failure",))
    assert code == 1
    finding = report["outcomes"][0]
    original = json.loads(Path(finding["original"]).read_text())
    assert original["input"] == [8]
    assert finding["verification"] == "confirmed"
    assert finding["derived"]
    assert all(Path(path).exists() for path in finding["derived"])


@pytest.mark.skipif(
    os.name == "nt", reason="POSIX signal delivery; Windows cancellation has its own probe"
)
def test_interrupt_produces_report(process_project: Path) -> None:
    """An orderly parent interrupt is not reported as a target timeout."""
    config = process_project / "interrupt.json"
    out = process_project / "interrupt-out"
    config.write_text(
        json.dumps(
            {
                "python": sys.executable,
                "pytest_args": [str(process_project), "-q", "-s"],
                "invocations": [{"test_id": "test_process.py::test_hang", "inputs": [[8]]}],
                "limits": {"allow_best_effort": True},
            }
        )
    )
    with subprocess.Popen(  # noqa: S603 -- owned probe/config
        [str(BINARY), "run", "--config", str(config), "--out", str(out)],
        cwd=process_project,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    ) as parent:
        time.sleep(0.6)
        parent.send_signal(signal.SIGINT)
        assert parent.wait(timeout=10) == INTERRUPTED_EXIT
    report = cast("Report", json.loads((out / "report.json").read_text()))
    assert report["interrupted"] is True


def test_crash_during_shrink_retains_original_and_active_token(
    probe_run: Callable[..., tuple[int, Report]],
) -> None:
    """Shrinker candidate acknowledgment precedes even fatal target execution."""
    code, report = probe_run(("shrink_crash",))
    assert code == 1
    finding = report["outcomes"][0]
    original_bytes = Path(finding["original"]).read_bytes()
    evidence = [json.loads(Path(path).read_text()) for path in finding["derived"]]
    assert any(item["evidence"].get("active_token") for item in evidence)
    assert any(
        item["evidence"].get("outcome", {}).get("kind") == "native_crash" for item in evidence
    )
    assert Path(finding["original"]).read_bytes() == original_bytes


def test_exhausted_budget_preserves_pending_original(
    probe_run: Callable[..., tuple[int, Report]],
) -> None:
    """A replay terminated by the total deadline leaves the original pending."""
    code, report = probe_run(("setup_wait",), campaign_ms=3000)
    assert code == 1
    finding = report["outcomes"][0]
    assert finding["original"]
    assert Path(finding["original"]).exists()
    assert finding["verification"] == "pending"
    assert report["outcomes"][-1]["kind"] == "campaign_cancelled"


def test_setup_failure_is_infrastructure(probe_run: Callable[..., tuple[int, Report]]) -> None:
    """Fixture setup failures have no active candidate and cannot be target findings."""
    code, report = probe_run(("setup_error", "ok"))
    assert code == 2  # noqa: PLR2004 -- documented infrastructure exit
    assert report["outcomes"][0]["kind"] == "infrastructure"
    assert report["outcomes"][0]["original"] is None
    assert report["outcomes"][-1]["kind"] == "pass"


@pytest.mark.skipif(os.name == "nt", reason="trusted POSIX descriptor fault injection")
def test_malformed_control_is_infrastructure(probe_run: Callable[..., tuple[int, Report]]) -> None:
    """A malformed frame is never classified as a crash, assertion or success."""
    code, report = probe_run(("corrupt",))
    assert code == 2  # noqa: PLR2004 -- documented infrastructure exit
    assert report["outcomes"][0]["kind"] == "infrastructure"


@pytest.mark.skipif(os.name == "nt", reason="trusted POSIX descriptor fault injection")
def test_closed_control_cannot_block_parent(probe_run: Callable[..., tuple[int, Report]]) -> None:
    """EOF while the target remains alive is infrastructure, with bounded cleanup."""
    started = time.monotonic()
    code, report = probe_run(("close_control",))
    assert code == 2  # noqa: PLR2004 -- documented infrastructure exit
    assert report["outcomes"][0]["kind"] == "infrastructure"
    assert time.monotonic() - started < 1.5  # noqa: PLR2004 -- less than target's two-second wait


def test_pytest_fail_is_an_oracle_failure(probe_run: Callable[..., tuple[int, Report]]) -> None:
    """pytest.fail inherits BaseException and must retain ordinary test semantics."""
    code, report = probe_run(("pytest_failure",))
    assert code == 1
    assert report["outcomes"][0]["kind"] == "failure"
    assert report["outcomes"][0]["verification"] == "confirmed"


@pytest.mark.skipif(os.name == "nt", reason="trusted POSIX descriptor fault injection")
def test_invalid_result_still_keeps_active_input(
    probe_run: Callable[..., tuple[int, Report]],
) -> None:
    """Correlation validation must complete before the parent releases its buffer."""
    code, report = probe_run(("wrong_id",))
    assert code == 2  # noqa: PLR2004 -- documented infrastructure exit
    outcome = report["outcomes"][0]
    assert outcome["kind"] == "infrastructure"
    assert json.loads(Path(outcome["original"]).read_text())["input"] == [8]
