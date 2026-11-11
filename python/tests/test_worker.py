# Copyright (c) 2026 kiryl contributors
"""Worker control exchanges acknowledge ownership before executing another case."""

import ctypes
import io
import site
import subprocess
import sys
from collections.abc import Callable, Iterator  # noqa: TC003 -- pytest introspects annotations
from contextlib import nullcontext
from pathlib import Path  # noqa: TC003 -- pytest introspects annotations
from types import SimpleNamespace
from typing import BinaryIO
from unittest.mock import Mock

import adapter
import pytest
import runtime
import worker
from adapter import CaseResult, SessionEvidence
from wire import receive, send
from worker import Driver, _bootstrap, _dispatch


@pytest.fixture
def driver_transport(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> Iterator[tuple[Driver, BinaryIO, io.BytesIO]]:
    """Reserve a real descriptor while keeping protocol traffic in inspectable streams."""
    incoming = (tmp_path / "control.bin").open("w+b")
    text = io.TextIOWrapper(incoming)
    monkeypatch.setattr(sys, "stdin", text)
    output = io.BytesIO()
    driver = Driver(output)
    try:
        yield driver, incoming, output
    finally:
        driver.input.close()
        text.close()


def queue(stream: BinaryIO, value: dict[str, object]) -> None:
    """Write one controller response and rewind its shared descriptor."""
    send(stream, value)
    stream.seek(0)


def test_driver_stops_on_end(driver_transport: tuple[Driver, BinaryIO, io.BytesIO]) -> None:
    """An end frame closes a target invocation without manufacturing input."""
    driver, incoming, output = driver_transport
    queue(incoming, {"kind": "end"})
    assert driver.next_case("target") is None
    output.seek(0)
    assert receive(output) == {"version": 1, "kind": "next", "test_id": "target"}


def test_driver_rejects_unexpected_response(
    driver_transport: tuple[Driver, BinaryIO, io.BytesIO],
) -> None:
    """Unexpected controller messages cannot be interpreted as candidates."""
    driver, incoming, _ = driver_transport
    queue(incoming, {"kind": "ack"})
    with pytest.raises(ValueError, match="unexpected controller"):
        driver.next_case("target")


def test_driver_accepts_owned_candidate(
    driver_transport: tuple[Driver, BinaryIO, io.BytesIO],
) -> None:
    """Retain the controller's case identifier alongside its exact candidate bytes."""
    driver, incoming, _ = driver_transport
    queue(incoming, {"kind": "case", "case_id": 7, "input": [0, 255]})
    assert driver.next_case("target") == b"\x00\xff"
    assert driver.case_id == 7  # noqa: PLR2004 -- independent controller identifier


@pytest.mark.parametrize("reply", ["ack", "unexpected"])
def test_ordinary_case_requires_acknowledgment(
    driver_transport: tuple[Driver, BinaryIO, io.BytesIO], reply: str
) -> None:
    """Ordinary execution must receive ownership acknowledgment before returning."""
    driver, incoming, output = driver_transport
    queue(incoming, {"kind": reply})
    outcomes = {
        "ack": nullcontext(),
        "unexpected": pytest.raises(ValueError, match="ordinary-case"),
    }
    with outcomes[reply]:
        driver.begin_ordinary("target")
    output.seek(0)
    assert receive(output) == {
        "version": 1,
        "kind": "baseline_case",
        "test_id": "target",
        "case_id": 1,
    }


@pytest.fixture
def acknowledged_driver(
    driver_transport: tuple[Driver, BinaryIO, io.BytesIO],
) -> tuple[Driver, io.BytesIO]:
    """Queue a successful parent acknowledgment for one observation."""
    driver, incoming, output = driver_transport
    queue(incoming, {"kind": "ack"})
    return driver, output


@pytest.mark.parametrize(("kind", "frame_kind"), [("pass", "result"), ("skip", "skipped")])
def test_driver_serializes_observation_bytes(
    acknowledged_driver: tuple[Driver, io.BytesIO], kind: str, frame_kind: str
) -> None:
    """Byte fields become JSON arrays while skip outcomes retain their frame type."""
    driver, output = acknowledged_driver
    driver.observe(CaseResult("target", kind, canonical=b"\x00", replay_token=b"\xff"))
    output.seek(0)
    message = receive(output)
    assert message["kind"] == frame_kind
    assert message["observation"] == {
        "nodeid": "target",
        "kind": kind,
        "canonical": [0],
        "fingerprint": None,
        "replay_token": [255],
        "rendered": None,
        "verification": "pending",
        "database_replay": False,
        "features": [],
        "elapsed_ns": 0,
    }


def test_driver_rejects_missing_finding_ack(
    driver_transport: tuple[Driver, BinaryIO, io.BytesIO],
) -> None:
    """The worker cannot proceed until finding persistence has been acknowledged."""
    driver, incoming, _ = driver_transport
    queue(incoming, {"kind": "end"})
    with pytest.raises(ValueError, match="finding acknowledgment"):
        driver.observe(CaseResult("target", "pass"))


@pytest.fixture
def dispatch_transport(monkeypatch: pytest.MonkeyPatch) -> tuple[Mock, io.BytesIO]:
    """Replace descriptor acquisition while retaining real frame serialization."""
    driver = Mock()
    driver.input = io.BytesIO()
    send(driver.input, {"kind": "ack"})
    driver.input.seek(0)
    monkeypatch.setattr(worker, "Driver", Mock(return_value=driver))
    return driver, io.BytesIO()


@pytest.mark.parametrize("kind", ["pass", "infrastructure"])
def test_dispatch_run_reports_infrastructure(
    dispatch_transport: tuple[Mock, io.BytesIO], monkeypatch: pytest.MonkeyPatch, kind: str
) -> None:
    """Each completed run sends done, and infrastructure receives its own error frame."""
    _, output = dispatch_transport
    run = Mock(return_value=SessionEvidence(cases=[CaseResult("target", kind)]))
    monkeypatch.setattr(adapter, "run_session", run)
    _dispatch(output, {"test_id": "target", "pytest_args": []})
    output.seek(0)
    expected = {"pass": "done", "infrastructure": "error"}
    assert receive(output)["kind"] == expected[kind]
    run.assert_called_once()


def test_dispatch_baseline_publishes_statistics(
    dispatch_transport: tuple[Mock, io.BytesIO], monkeypatch: pytest.MonkeyPatch
) -> None:
    """Ordinary Hypothesis execution returns its exit and measured statistics."""
    driver, output = dispatch_transport
    run = Mock(return_value=(1, {"useful": 3}))
    monkeypatch.setattr(adapter, "run_ordinary", run)
    _dispatch(output, {"mode": "baseline", "test_id": "target", "pytest_args": []})
    output.seek(0)
    assert receive(output) == {
        "version": 1,
        "kind": "baseline_done",
        "exit_code": 1,
        "statistics": {"useful": 3},
    }
    run.assert_called_once_with([], driver.begin_ordinary, driver.observe)
    assert receive(output)["kind"] == "done"


def replay_case(
    nodeid: str, token: bytes, args: list[str], before: Callable[[], None]
) -> CaseResult:
    """Emulate replay's pre-execution acknowledgment boundary."""
    assert token == b"\x01"
    assert args == []
    before()
    return CaseResult(nodeid, "failure")


def shrink_case(
    nodeid: str, token: bytes, budget: float, before: Callable[[bytes], None]
) -> SimpleNamespace:
    """Emulate publishing a shrink candidate before its execution."""
    assert nodeid == "target"
    assert token == b"\x01"
    assert budget == 1.0
    before(b"\x02")
    return SimpleNamespace(best=b"\x02", complete=True)


@pytest.mark.parametrize("reply", ["ack", "bad"])
def test_dispatch_replay_requires_case_ack(
    dispatch_transport: tuple[Mock, io.BytesIO], monkeypatch: pytest.MonkeyPatch, reply: str
) -> None:
    """A replay candidate stays owned by the parent before its oracle runs."""
    driver, output = dispatch_transport
    driver.input = io.BytesIO()
    queue(driver.input, {"kind": reply})
    monkeypatch.setattr(adapter, "run_token", replay_case)
    outcomes = {"ack": nullcontext(), "bad": pytest.raises(ValueError, match="replay-case")}
    with outcomes[reply]:
        _dispatch(output, {"mode": "replay", "test_id": "target", "pytest_args": [], "token": [1]})
    output.seek(0)
    assert receive(output)["kind"] == "replay_case"


@pytest.mark.parametrize("reply", ["ack", "bad"])
def test_dispatch_minimize_requires_candidate_ack(
    dispatch_transport: tuple[Mock, io.BytesIO], monkeypatch: pytest.MonkeyPatch, reply: str
) -> None:
    """Shrinking cannot evaluate a candidate before the controller sees its bytes."""
    driver, output = dispatch_transport
    driver.input = io.BytesIO()
    queue(driver.input, {"kind": reply})
    monkeypatch.setattr(adapter, "minimize", shrink_case)
    outcomes = {"ack": nullcontext(), "bad": pytest.raises(ValueError, match="shrink candidate")}
    with outcomes[reply]:
        _dispatch(
            output,
            {
                "mode": "minimize",
                "test_id": "target",
                "pytest_args": [],
                "token": [1],
                "budget_seconds": 1.0,
            },
        )
    output.seek(0)
    assert receive(output) == {"version": 1, "kind": "candidate", "token": [2]}


def test_dispatch_rejects_unknown_mode(dispatch_transport: tuple[Mock, io.BytesIO]) -> None:
    """Unsupported operations must not become successful empty sessions."""
    _, output = dispatch_transport
    with pytest.raises(ValueError, match="unknown worker mode"):
        _dispatch(output, {"mode": "unknown", "test_id": "target", "pytest_args": []})


def test_main_reports_runtime_rejection(monkeypatch: pytest.MonkeyPatch) -> None:
    """An admitted control stream receives runtime errors before adapter dispatch."""
    output = io.BytesIO()
    monkeypatch.setattr(worker, "_bootstrap", Mock(return_value=(output, {})))
    monkeypatch.setattr(runtime, "validate_runtime", Mock(side_effect=RuntimeError("unsupported")))
    worker.main()
    output.seek(0)
    assert receive(output) == {"version": 1, "kind": "error", "message": "unsupported"}


def test_main_reports_skip_from_dispatch(
    monkeypatch: pytest.MonkeyPatch, dispatch_transport: tuple[Mock, io.BytesIO]
) -> None:
    """A pytest skip escaping dispatch is an explicit skip followed by done."""
    driver, output = dispatch_transport
    monkeypatch.setattr(worker, "_bootstrap", Mock(return_value=(output, {"test_id": "target"})))
    monkeypatch.setattr(worker, "_dispatch", Mock(side_effect=pytest.skip.Exception("skip")))
    worker.main()
    driver.observe.assert_called_once_with(CaseResult("target", "skip"))
    output.seek(0)
    assert receive(output)["kind"] == "done"


@pytest.fixture
def bootstrap_streams(monkeypatch: pytest.MonkeyPatch) -> tuple[io.BytesIO, io.BytesIO]:
    """Substitute owned descriptors so bootstrap cannot redirect the test runner."""
    incoming = io.BytesIO()
    output = io.BytesIO()
    monkeypatch.setattr(sys, "stdin", SimpleNamespace(buffer=incoming))
    monkeypatch.setattr(sys, "stdout", SimpleNamespace(fileno=lambda: 1))
    monkeypatch.setattr(sys, "stderr", SimpleNamespace(fileno=lambda: 2))
    monkeypatch.setattr(worker.os, "dup", Mock(return_value=97))
    monkeypatch.setattr(worker.os, "dup2", Mock())
    monkeypatch.setattr(worker.os, "fdopen", Mock(return_value=output))
    monkeypatch.setattr(site, "main", Mock())
    monkeypatch.setattr(worker, "disable_linux_core_dumps", Mock())
    return incoming, output


@pytest.mark.parametrize("platform", ["darwin", "linux", "win32"])
def test_bootstrap_reserves_streams_before_platform_setup(
    bootstrap_streams: tuple[io.BytesIO, io.BytesIO], monkeypatch: pytest.MonkeyPatch, platform: str
) -> None:
    """Native setup follows the controller's start acknowledgment on every platform."""
    incoming, output = bootstrap_streams
    queue(incoming, {"kind": "start"})
    monkeypatch.setattr(sys, "platform", platform)
    monkeypatch.setattr(ctypes, "WinDLL", Mock(), raising=False)
    monkeypatch.setattr(ctypes, "CDLL", Mock())
    control, request = _bootstrap()
    assert control is output
    assert request == {"version": 1, "kind": "start"}
    output.seek(0)
    assert receive(output)["kind"] == "ready"


def test_bootstrap_rejects_missing_start_ack(
    bootstrap_streams: tuple[io.BytesIO, io.BytesIO],
) -> None:
    """Unacknowledged sessions cannot load site packages or execute target code."""
    incoming, _ = bootstrap_streams
    queue(incoming, {"kind": "ack"})
    with pytest.raises(ValueError, match="session was not acknowledged"):
        _bootstrap()


def test_script_entry_point_rejects_unacknowledged_session() -> None:
    """Executing the bootstrap file dispatches main and preserves the framed ready response."""
    incoming = io.BytesIO()
    send(incoming, {"kind": "ack"})
    result = subprocess.run(  # noqa: S603 -- owned worker and invalid controller response
        [sys.executable, worker.__file__],
        input=incoming.getvalue(),
        capture_output=True,
        check=False,
        timeout=10,
    )
    assert result.returncode == 1
    assert receive(io.BytesIO(result.stdout))["kind"] == "ready"
    assert b"session was not acknowledged" in result.stderr
