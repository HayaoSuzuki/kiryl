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

from model import ArtifactMode

if TYPE_CHECKING:
    from collections.abc import Callable

LOG_LIMIT = 1024
INTERRUPTED_EXIT = 130
TWO_CASES = 2
DISCOVERY_REPLAY_SHRINK = 3


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
    useful: int


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
def blocked_receive():
    import worker
    original = worker.receive
    def blocked(stream):
        time.sleep(2)
        return {}
    worker.receive = blocked
    yield
    worker.receive = original

@settings(suppress_health_check=[HealthCheck.function_scoped_fixture])
@given(st.integers(0, 255))
def test_blocked_input(blocked_receive, value):
    pass

@pytest.fixture
def fatal_teardown():
    yield
    os.abort()

@settings(suppress_health_check=[HealthCheck.function_scoped_fixture])
@given(st.integers(0, 255))
def test_failure_then_fatal_teardown(fatal_teardown, value):
    assert False

@pytest.fixture
def slow_shrink_ack():
    import hypothesis_bridge
    original = hypothesis_bridge.BoundTarget.minimize
    def delayed(self, token, budget_seconds, before_case):
        def acknowledged(candidate):
            before_case(candidate)
            time.sleep(0.6)
        return original(self, token, budget_seconds, acknowledged)
    hypothesis_bridge.BoundTarget.minimize = delayed
    yield
    hypothesis_bridge.BoundTarget.minimize = original

@settings(suppress_health_check=[HealthCheck.function_scoped_fixture])
@given(st.integers(0, 255))
def test_unlimited_shrink(slow_shrink_ack, value):
    assert value < 1

@pytest.fixture
def slow_setup():
    time.sleep(0.3)
    yield
    open("slow-teardown.txt", "a").write("done\\n")

@settings(suppress_health_check=[HealthCheck.function_scoped_fixture])
@given(st.integers(0, 255))
def test_fast_failure(slow_setup, value):
    assert False

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
def test_crash_later(value):
    if value:
        os.abort()

@given(st.integers(0, 255))
def test_hang_later(value):
    if value:
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
        search: bool = False,
        feedback: bool = True,
        baseline: bool = False,
    ) -> tuple[int, Report]:
        nonlocal serial
        serial += 1
        out = process_project / f"out-{serial}"
        config = {
            "python": sys.executable,
            "pytest_args": [
                str(process_project / "test_process.py") + f"::test_{names[0]}"
                if baseline
                else str(process_project),
                "-q",
                "-s",
            ],
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
            "search": search,
            "feedback": feedback,
            "baseline": baseline,
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


def test_blocked_input_writer_cannot_hide_campaign_deadline(
    probe_run: Callable[..., tuple[int, Report]],
) -> None:
    """A real worker can request input then stop reading; parent deadlines still apply."""
    supplied = b"x" * 65536
    code, report = probe_run(("blocked_input",), inputs=(supplied,), campaign_ms=700)
    assert code == 1
    outcome = report["outcomes"][0]
    assert outcome["kind"] == "campaign_cancelled"
    assert json.loads(Path(outcome["original"]).read_text())["input"] == list(supplied)


def test_finding_does_not_hide_fatal_teardown(probe_run: Callable[..., tuple[int, Report]]) -> None:
    """A saved oracle failure cannot turn incomplete pytest teardown into success."""
    code, report = probe_run(("failure_then_fatal_teardown", "ok"))
    assert code == 2  # noqa: PLR2004 -- documented infrastructure exit
    assert report["outcomes"][0]["kind"] == "failure"
    assert Path(report["outcomes"][0]["original"]).exists()
    assert any(outcome["kind"] == "infrastructure" for outcome in report["outcomes"])
    assert report["outcomes"][-1]["kind"] == "pass"


def test_non_object_config_reports_infrastructure(tmp_path: Path) -> None:
    """Malformed configuration is a controlled error with a durable report."""
    config = tmp_path / "invalid.json"
    config.write_text("[]")
    out = tmp_path / "out"
    completed = subprocess.run(  # noqa: S603 -- owned probe and invalid local config
        [str(BINARY), "run", "--config", str(config), "--out", str(out)],
        capture_output=True,
        timeout=5,
        check=False,
    )
    assert completed.returncode == 2  # noqa: PLR2004 -- documented infrastructure exit
    assert json.loads((out / "report.json").read_text())["outcomes"][0]["kind"] == "infrastructure"


def _minimization_record(value: dict[str, object]) -> bool:
    """Select one process-boundary minimization record from stored wire evidence."""
    return ArtifactMode(value["mode"]) == ArtifactMode.MINIMIZE


def _retained_original(value: Outcome) -> bool:
    """Select the finding carrying immutable original bytes in a report."""
    return bool(value["original"])


def test_unlimited_campaign_has_no_hidden_one_second_shrink_budget(
    probe_run: Callable[..., tuple[int, Report]],
) -> None:
    """No campaign limit permits a semantic shrink spanning multiple slow acknowledgments."""
    code, report = probe_run(("unlimited_shrink",), campaign_ms=None, case_ms=2500)
    assert code == 1
    finding = report["outcomes"][0]
    records = [json.loads(Path(path).read_text())["evidence"] for path in finding["derived"]]
    minimized = list(filter(_minimization_record, records))
    assert minimized
    assert minimized[0]["complete"] is True


@pytest.mark.parametrize("feedback", [True, False])
@pytest.mark.parametrize(
    ("target", "minimum_attempts"),
    [("crash", 1), ("hang", 1), ("crash_later", 2), ("hang_later", 2)],
)
@pytest.mark.parametrize("scheduled", [(), (b"\x00",)])
def test_generated_finding_replays_retained_input(
    probe_run: Callable[..., tuple[int, Report]],
    target: str,
    minimum_attempts: int,
    scheduled: tuple[bytes, ...],
    *,
    feedback: bool,
) -> None:
    """Generated bytes, absent from the input schedule, survive fresh raw replay."""
    code, report = probe_run(
        (target,), inputs=scheduled, search=True, feedback=feedback, case_ms=150
    )
    assert code == 1
    finding = next(filter(_retained_original, report["outcomes"]))
    assert finding["verification"] == "confirmed"
    original = json.loads(Path(finding["original"]).read_text())
    assert original["input"]
    assert report["attempts"] >= minimum_attempts
    derived = json.loads(Path(finding["derived"][0]).read_text())
    assert derived["evidence"]["input"] == original["input"]


def test_replay_case_budget_excludes_fixture_setup(
    probe_run: Callable[..., tuple[int, Report]], process_project: Path
) -> None:
    """All fresh executions reach the oracle despite setup exceeding case time."""
    code, report = probe_run(("fast_failure",), case_ms=150, campaign_ms=5000)
    assert code == 1
    finding = report["outcomes"][0]
    assert finding["verification"] == "confirmed"
    assert (
        len((process_project / "slow-teardown.txt").read_text().splitlines())
        >= DISCOVERY_REPLAY_SHRINK
    )
    records = [json.loads(Path(path).read_text())["evidence"] for path in finding["derived"]]
    assert any(record["mode"] == "minimize" and "token" in record for record in records)


@pytest.mark.parametrize("baseline", [False, True])
@pytest.mark.parametrize("name", ["unit", "pytest", "marked", "setup"])
def test_parent_skips_are_explicit_without_findings(
    probe_run: Callable[..., tuple[int, Report]],
    process_project: Path,
    name: str,
    *,
    baseline: bool,
) -> None:
    """Skip frames settle active work or report setup skips without inventing inputs."""
    target = process_project / "test_process.py"
    target.write_text(
        target.read_text()
        + """
import unittest
@pytest.fixture
def skipped_setup():
    pytest.skip("setup")
@given(st.just(1))
def test_skip_unit(value):
    raise unittest.SkipTest("unit")
@given(st.just(1))
def test_skip_pytest(value):
    pytest.skip("body")
@pytest.mark.skip(reason="marker")
@given(st.just(1))
def test_skip_marked(value):
    assert False
@settings(suppress_health_check=[HealthCheck.function_scoped_fixture])
@given(st.just(1))
def test_skip_setup(skipped_setup, value):
    assert False
"""
    )
    code, report = probe_run((f"skip_{name}",), baseline=baseline)
    assert code == 0
    assert [outcome["kind"] for outcome in report["outcomes"]] == ["skip"]
    assert not report["outcomes"][0]["original"]
    assert report["useful"] == 0


@pytest.mark.skipif(os.name == "nt", reason="POSIX reserved-FD fault injection")
def test_done_cannot_silently_discard_active_case(
    probe_run: Callable[..., tuple[int, Report]], process_project: Path
) -> None:
    """A completion frame requires a settled target observation."""
    target = process_project / "test_process.py"
    target.write_text(
        target.read_text()
        + """
@given(st.just(1))
def test_early_done(value):
    import json
    import struct
    encoded = json.dumps({"version": 1, "kind": "done"}).encode()
    os.write(3, struct.pack(">I", len(encoded)) + encoded)
    time.sleep(2)
"""
    )
    code, report = probe_run(("early_done",))
    assert code == 2  # noqa: PLR2004 -- infrastructure failure
    assert report["outcomes"][0]["kind"] == "infrastructure"
    assert report["outcomes"][0]["original"]


@pytest.mark.skipif(sys.platform != "linux", reason="Linux per-worker dumpability")
def test_linux_worker_disables_piped_core_handlers(
    probe_run: Callable[..., tuple[int, Report]], process_project: Path
) -> None:
    """The attached target observes PR_GET_DUMPABLE=0 before a native crash."""
    target = process_project / "test_process.py"
    target.write_text(
        target.read_text()
        + """
@given(st.just(1))
def test_dump_policy(value):
    import ctypes
    from pathlib import Path
    library = ctypes.CDLL(None)
    assert library.prctl(3, 0, 0, 0, 0) == 0
    Path("dump-policy.txt").write_text("disabled")
    os.abort()
"""
    )
    code, report = probe_run(("dump_policy",), case_ms=500)
    assert code == 1
    assert (process_project / "dump-policy.txt").read_text() == "disabled"
    finding = report["outcomes"][0]
    assert finding["kind"] == "native_crash"
    assert finding["verification"] == "confirmed"
