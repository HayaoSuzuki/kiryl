# Copyright (c) 2026 kiryl contributors
"""Worker control exchanges acknowledge ownership before executing another case."""

import ctypes
import io
import site
import subprocess
import sys
from collections.abc import Callable, Iterator  # noqa: TC003 -- pytest introspects annotations
from contextlib import ExitStack, nullcontext
from pathlib import Path  # noqa: TC003 -- pytest introspects annotations
from types import ModuleType, SimpleNamespace
from typing import BinaryIO, Protocol, cast
from unittest.mock import Mock, create_autospec, patch

import adapter
import pytest
import runtime
import worker
from adapter import CaseResult, SessionEvidence
from model import ResultKind
from wire import receive, send
from worker import Driver, _bootstrap, _dispatch


class ErrorModeAPI(Protocol):
    """Declare kernel32.SetErrorMode's documented one-argument signature."""

    def __call__(self, mode: int) -> int:
        """Set the process error mode using the Windows ABI contract."""
        ...


class AbortBehaviorAPI(Protocol):
    """Declare the CRT abort behavior function and mutable ctypes descriptors."""

    argtypes: object = None
    restype: object = None

    def __call__(self, flags: int, mask: int) -> int:
        """Set selected CRT abort flags."""
        ...


class ResourceLimitAPI(Protocol):
    """Declare resource.setrlimit on hosts without the resource module."""

    @staticmethod
    def setrlimit(resource: int, limits: tuple[int, int]) -> None:
        """Set the soft and hard limits for one native resource."""
        ...


def resource_limit_api() -> Callable[..., object]:
    """Select the real native API when available, otherwise its documented signature."""
    try:
        import resource  # noqa: PLC0415 -- unavailable on Windows
    except ImportError:
        return ResourceLimitAPI.setrlimit
    return cast("Callable[..., object]", getattr(resource, "setrlimit", ResourceLimitAPI.setrlimit))


@pytest.fixture
def patches() -> Iterator[ExitStack]:
    """Restore every standard-library patch when its test scenario ends."""
    with ExitStack() as stack:
        yield stack


@pytest.fixture
def driver_transport(
    tmp_path: Path, patches: ExitStack
) -> Iterator[tuple[Driver, BinaryIO, io.BytesIO]]:
    """Reserve a real descriptor while keeping protocol traffic in inspectable streams."""
    incoming = (tmp_path / "control.bin").open("w+b")
    text = io.TextIOWrapper(incoming)
    patches.enter_context(patch.object(sys, "stdin", text))
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


@pytest.mark.parametrize(
    ("kind", "frame_kind"), [(ResultKind.PASS, "result"), (ResultKind.SKIP, "skipped")]
)
def test_driver_serializes_observation_bytes(
    acknowledged_driver: tuple[Driver, io.BytesIO], kind: ResultKind, frame_kind: str
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
        driver.observe(CaseResult("target", ResultKind.PASS))


@pytest.fixture
def dispatch_transport(
    patches: ExitStack, driver_transport: tuple[Driver, BinaryIO, io.BytesIO]
) -> tuple[Mock, io.BytesIO]:
    """Replace descriptor acquisition while retaining real frame serialization."""
    original_driver, _, _ = driver_transport
    driver = cast("Mock", create_autospec(original_driver, spec_set=True))
    driver.input = io.BytesIO()
    send(driver.input, {"kind": "ack"})
    driver.input.seek(0)
    patches.enter_context(
        patch.object(worker, "Driver", create_autospec(Driver, spec_set=True, return_value=driver))
    )
    return driver, io.BytesIO()


@pytest.mark.parametrize("kind", [ResultKind.PASS, ResultKind.INFRASTRUCTURE])
def test_dispatch_run_reports_infrastructure(
    dispatch_transport: tuple[Mock, io.BytesIO], patches: ExitStack, kind: ResultKind
) -> None:
    """Each completed run sends done, and infrastructure receives its own error frame."""
    _, output = dispatch_transport
    run = create_autospec(
        adapter.run_session,
        spec_set=True,
        return_value=SessionEvidence(cases=[CaseResult("target", kind)]),
    )
    patches.enter_context(patch.object(adapter, "run_session", run))
    _dispatch(output, {"test_id": "target", "pytest_args": []})
    output.seek(0)
    expected = {"pass": "done", "infrastructure": "error"}
    assert receive(output)["kind"] == expected[kind]
    run.assert_called_once()


def test_dispatch_baseline_publishes_statistics(
    dispatch_transport: tuple[Mock, io.BytesIO], patches: ExitStack
) -> None:
    """Ordinary Hypothesis execution returns its exit and measured statistics."""
    driver, output = dispatch_transport
    run = create_autospec(adapter.run_ordinary, spec_set=True, return_value=(1, {"useful": 3}))
    patches.enter_context(patch.object(adapter, "run_ordinary", run))
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
    return CaseResult(nodeid, ResultKind.FAILURE)


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
    dispatch_transport: tuple[Mock, io.BytesIO], patches: ExitStack, reply: str
) -> None:
    """A replay candidate stays owned by the parent before its oracle runs."""
    driver, output = dispatch_transport
    driver.input = io.BytesIO()
    queue(driver.input, {"kind": reply})
    patches.enter_context(
        patch.object(
            adapter,
            "run_token",
            create_autospec(adapter.run_token, spec_set=True, side_effect=replay_case),
        )
    )
    outcomes = {"ack": nullcontext(), "bad": pytest.raises(ValueError, match="replay-case")}
    with outcomes[reply]:
        _dispatch(output, {"mode": "replay", "test_id": "target", "pytest_args": [], "token": [1]})
    output.seek(0)
    assert receive(output)["kind"] == "replay_case"


@pytest.mark.parametrize("reply", ["ack", "bad"])
def test_dispatch_minimize_requires_candidate_ack(
    dispatch_transport: tuple[Mock, io.BytesIO], patches: ExitStack, reply: str
) -> None:
    """Shrinking cannot evaluate a candidate before the controller sees its bytes."""
    driver, output = dispatch_transport
    driver.input = io.BytesIO()
    queue(driver.input, {"kind": reply})
    patches.enter_context(
        patch.object(
            adapter,
            "minimize",
            create_autospec(adapter.minimize, spec_set=True, side_effect=shrink_case),
        )
    )
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


def test_main_reports_runtime_rejection(patches: ExitStack) -> None:
    """An admitted control stream receives runtime errors before adapter dispatch."""
    output = io.BytesIO()
    patches.enter_context(
        patch.object(
            worker,
            "_bootstrap",
            create_autospec(_bootstrap, spec_set=True, return_value=(output, {})),
        )
    )
    patches.enter_context(
        patch.object(
            runtime,
            "validate_runtime",
            create_autospec(
                runtime.validate_runtime, spec_set=True, side_effect=RuntimeError("unsupported")
            ),
        )
    )
    worker.main()
    output.seek(0)
    assert receive(output) == {"version": 1, "kind": "error", "message": "unsupported"}


def test_main_reports_skip_from_dispatch(
    patches: ExitStack, dispatch_transport: tuple[Mock, io.BytesIO]
) -> None:
    """A pytest skip escaping dispatch is an explicit skip followed by done."""
    driver, output = dispatch_transport
    patches.enter_context(
        patch.object(
            worker,
            "_bootstrap",
            create_autospec(
                _bootstrap, spec_set=True, return_value=(output, {"test_id": "target"})
            ),
        )
    )
    patches.enter_context(
        patch.object(
            worker,
            "_dispatch",
            create_autospec(_dispatch, spec_set=True, side_effect=pytest.skip.Exception("skip")),
        )
    )
    worker.main()
    driver.observe.assert_called_once_with(CaseResult("target", ResultKind.SKIP))
    output.seek(0)
    assert receive(output)["kind"] == "done"


@pytest.fixture
def bootstrap_streams(patches: ExitStack) -> tuple[io.BytesIO, io.BytesIO]:
    """Substitute owned descriptors so bootstrap cannot redirect the test runner."""
    incoming = io.BytesIO()
    output = io.BytesIO()
    patches.enter_context(patch.object(sys, "stdin", SimpleNamespace(buffer=incoming)))
    patches.enter_context(
        patch.object(
            sys,
            "stdout",
            SimpleNamespace(
                fileno=create_autospec(sys.stdout.fileno, spec_set=True, return_value=1)
            ),
        )
    )
    patches.enter_context(
        patch.object(
            sys,
            "stderr",
            SimpleNamespace(
                fileno=create_autospec(sys.stderr.fileno, spec_set=True, return_value=2)
            ),
        )
    )
    patches.enter_context(
        patch.object(
            worker.os, "dup", create_autospec(worker.os.dup, spec_set=True, return_value=97)
        )
    )
    patches.enter_context(
        patch.object(worker.os, "dup2", create_autospec(worker.os.dup2, spec_set=True))
    )
    patches.enter_context(
        patch.object(
            worker.os,
            "fdopen",
            create_autospec(worker.os.fdopen, spec_set=True, return_value=output),
        )
    )
    resource_probe = ModuleType("resource")
    patches.enter_context(
        patch.object(
            resource_probe,
            "setrlimit",
            create_autospec(resource_limit_api(), spec_set=True),
            create=True,
        )
    )
    patches.enter_context(patch.object(resource_probe, "RLIMIT_CORE", 4, create=True))
    patches.enter_context(patch.dict(sys.modules, {"resource": resource_probe}))
    patches.enter_context(patch.object(site, "main", create_autospec(site.main, spec_set=True)))
    patches.enter_context(
        patch.object(
            worker,
            "disable_linux_core_dumps",
            create_autospec(worker.disable_linux_core_dumps, spec_set=True),
        )
    )
    return incoming, output


@pytest.mark.parametrize("platform", ["darwin", "linux", "win32"])
def test_bootstrap_reserves_streams_before_platform_setup(
    bootstrap_streams: tuple[io.BytesIO, io.BytesIO], patches: ExitStack, platform: str
) -> None:
    """Native setup follows the controller's start acknowledgment on every platform."""
    incoming, output = bootstrap_streams
    queue(incoming, {"kind": "start"})
    patches.enter_context(patch.object(sys, "platform", platform))
    patches.enter_context(
        patch.object(
            ctypes,
            "WinDLL",
            create_autospec(
                getattr(ctypes, "WinDLL", ctypes.CDLL),
                spec_set=True,
                return_value=SimpleNamespace(
                    SetErrorMode=create_autospec(ErrorModeAPI, instance=True, spec_set=True)
                ),
            ),
            create=True,
        )
    )
    patches.enter_context(
        patch.object(
            ctypes,
            "CDLL",
            create_autospec(
                ctypes.CDLL,
                spec_set=True,
                return_value=SimpleNamespace(
                    _set_abort_behavior=create_autospec(
                        AbortBehaviorAPI, instance=True, spec_set=True
                    )
                ),
            ),
        )
    )
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


def test_resource_limit_spec_exists_without_native_module() -> None:
    """The documented signature permits bootstrap tests on hosts without resource."""
    with patch.dict(sys.modules, {"resource": None}):
        assert resource_limit_api() is ResourceLimitAPI.setrlimit


def test_infrastructure_diagnostic_preserves_wire_state_labels(
    dispatch_transport: tuple[Mock, io.BytesIO], patches: ExitStack
) -> None:
    """Human diagnostics expose stable outcome labels after typed execution."""
    _, output = dispatch_transport
    run = create_autospec(
        adapter.run_session,
        spec_set=True,
        return_value=SessionEvidence(cases=[CaseResult("target", ResultKind.INFRASTRUCTURE)]),
    )
    patches.enter_context(patch.object(adapter, "run_session", run))
    _dispatch(output, {"test_id": "target", "pytest_args": []})
    output.seek(0)
    message = receive(output)
    assert "'kind': 'infrastructure'" in str(message["message"])
    assert "'verification': 'pending'" in str(message["message"])
