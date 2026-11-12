# Copyright (c) 2026 kiryl contributors
"""Trusted product bootstrap; controller owns candidates and all persistence."""

import os
import sys
from dataclasses import asdict
from typing import TYPE_CHECKING, cast

from model import ControlKind, ResultKind, WorkerMode
from wire import receive, send

if TYPE_CHECKING:
    from typing import BinaryIO

    from adapter import CaseResult


def _control_kind(message: dict[str, object]) -> ControlKind | None:
    """Decode incoming labels; callers own operation-specific rejection messages."""
    try:
        return ControlKind(message["kind"])
    except ValueError:
        return None


def _observation(case: CaseResult) -> dict[str, object]:
    value = asdict(case)
    for key in ("canonical", "replay_token"):
        if value[key] is not None:
            value[key] = list(value[key])
    return cast("dict[str, object]", value)


class Driver:
    """Synchronous acknowledgments prevent a candidate escaping parent ownership."""

    def __init__(self, control: BinaryIO) -> None:
        """Own the reserved control stream for one worker lifetime."""
        self.control = control
        # Pytest capture redirects fd 0 as well as replacing sys.stdin.
        self.input = os.fdopen(os.dup(sys.stdin.fileno()), "rb", buffering=0)
        self.case_id = 0

    def next_case(self, nodeid: str) -> bytes | None:
        """Ask only after pytest has set up the target invocation."""
        send(self.control, {"kind": ControlKind.NEXT, "test_id": nodeid})
        message = receive(self.input)
        kind = _control_kind(message)
        if kind == ControlKind.END:
            return None
        if kind != ControlKind.CASE:
            error = "unexpected controller response"
            raise ValueError(error)
        self.case_id = cast("int", message["case_id"])
        return bytes(cast("list[int]", message["input"]))

    def begin_ordinary(self, nodeid: str) -> None:
        """Publish normal Hypothesis execution boundaries without requesting input."""
        self.case_id += 1
        send(
            self.control,
            {"kind": ControlKind.BASELINE_CASE, "test_id": nodeid, "case_id": self.case_id},
        )
        if _control_kind(receive(self.input)) != ControlKind.ACK:
            message = "missing ordinary-case acknowledgment"
            raise ValueError(message)

    def observe(self, result: CaseResult) -> None:
        """Wait until the parent has saved any original before continuing."""
        send(
            self.control,
            {
                "kind": ControlKind.SKIPPED
                if result.kind == ResultKind.SKIP
                else ControlKind.RESULT,
                "case_id": self.case_id,
                "observation": _observation(result),
            },
        )
        if _control_kind(receive(self.input)) != ControlKind.ACK:
            message = "missing finding acknowledgment"
            raise ValueError(message)


def disable_linux_core_dumps() -> None:
    """Disable this worker's dumps, including piped handlers ignoring RLIMIT_CORE."""
    import ctypes  # noqa: PLC0415 -- only after resource attachment

    library = ctypes.CDLL(None, use_errno=True)
    prctl = library.prctl
    prctl.argtypes = (ctypes.c_int,) + (ctypes.c_ulong,) * 4
    prctl.restype = ctypes.c_int
    if prctl(4, 0, 0, 0, 0) != 0:  # PR_SET_DUMPABLE, SUID_DUMP_DISABLE
        raise OSError(ctypes.get_errno(), "prctl(PR_SET_DUMPABLE)")


def _bootstrap() -> tuple[BinaryIO, dict[str, object]]:
    """Redirect FD-level stdout before importing pytest or target modules."""
    control = os.fdopen(os.dup(sys.stdout.fileno()), "wb", buffering=0)
    os.dup2(sys.stderr.fileno(), sys.stdout.fileno())
    if sys.platform != "win32":
        import resource  # noqa: PLC0415 -- unavailable on Windows

        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    send(
        control,
        {
            "kind": ControlKind.READY,
            "environment": {
                "platform": sys.platform,
                "python": list(sys.version_info[:2]),
                "implementation": sys.implementation.name,
            },
        },
    )
    request = receive(sys.stdin.buffer)
    if _control_kind(request) != ControlKind.START:
        message = "session was not acknowledged"
        raise ValueError(message)
    import site  # noqa: PLC0415 -- load environment only after resource attachment

    if sys.platform == "win32":
        import ctypes  # noqa: PLC0415 -- Windows CRT setup after resource attachment

        ctypes.WinDLL("kernel32").SetErrorMode(0x0001 | 0x0002 | 0x8000)
        # MSVC abort without report-fault/dialog behavior exits with code 3.
        # https://learn.microsoft.com/en-us/cpp/c-runtime-library/reference/abort
        abort_behavior = ctypes.CDLL("ucrtbase")._set_abort_behavior  # noqa: SLF001 -- documented CRT API
        abort_behavior.argtypes = (ctypes.c_uint, ctypes.c_uint)
        abort_behavior.restype = ctypes.c_uint
        abort_behavior(0, 3)
    if sys.platform == "linux":
        disable_linux_core_dumps()
    site.main()
    return control, request


def _operation_mode(value: object) -> WorkerMode:
    """Decode external operation labels while retaining the protocol diagnostic."""
    try:
        return WorkerMode(value)
    except ValueError as error:
        message = "unknown worker mode"
        raise ValueError(message) from error


def _dispatch(control: BinaryIO, request: dict[str, object]) -> None:
    """Run one attached operation."""
    from adapter import Invocation, minimize, run_ordinary, run_session, run_token  # noqa: PLC0415 -- after attachment

    nodeid = cast("str", request["test_id"])
    args = cast("list[str]", request["pytest_args"])
    mode = _operation_mode(request.get("mode", WorkerMode.RUN))
    driver = Driver(control)
    if mode == WorkerMode.RUN:
        evidence = run_session(args, [Invocation(nodeid, ())], driver)
        infrastructure = [
            {**asdict(case), "kind": case.kind.value, "verification": case.verification.value}
            for case in evidence.cases
            if case.kind == ResultKind.INFRASTRUCTURE
        ]
        if infrastructure:
            send(control, {"kind": ControlKind.ERROR, "message": str(infrastructure)[-4096:]})
    elif mode == WorkerMode.BASELINE:
        code, statistics = run_ordinary(args, driver.begin_ordinary, driver.observe)
        send(
            control,
            {"kind": ControlKind.BASELINE_DONE, "exit_code": code, "statistics": statistics},
        )
    elif mode == WorkerMode.REPLAY:
        token = bytes(cast("list[int]", request["token"]))

        def before_replay() -> None:
            send(control, {"kind": ControlKind.REPLAY_CASE, "test_id": nodeid})
            if _control_kind(receive(driver.input)) != ControlKind.ACK:
                message = "missing replay-case acknowledgment"
                raise ValueError(message)

        driver.observe(run_token(nodeid, token, args, before_replay))
    else:
        token = bytes(cast("list[int]", request["token"]))

        def before_case(candidate: bytes) -> None:
            send(control, {"kind": ControlKind.CANDIDATE, "token": list(candidate)})
            if _control_kind(receive(driver.input)) != ControlKind.ACK:
                message = "missing shrink candidate acknowledgment"
                raise ValueError(message)

        shrunk = minimize(nodeid, token, cast("float", request["budget_seconds"]), before_case)
        send(
            control,
            {"kind": ControlKind.SHRUNK, "token": list(shrunk.best), "complete": shrunk.complete},
        )
    send(control, {"kind": ControlKind.DONE})


def main() -> None:
    """Dispatch an attached worker operation without exposing target stdout."""
    control, request = _bootstrap()
    from runtime import validate_runtime  # noqa: PLC0415 -- after resource attachment

    try:
        validate_runtime()
    except RuntimeError as error:
        send(control, {"kind": ControlKind.ERROR, "message": str(error)})
        return
    from adapter import CaseResult  # noqa: PLC0415 -- admitted runtime
    from hypothesis.core import skip_exceptions_to_reraise  # noqa: PLC0415

    try:
        _dispatch(control, request)
    except skip_exceptions_to_reraise():
        Driver(control).observe(CaseResult(cast("str", request["test_id"]), ResultKind.SKIP))
        send(control, {"kind": ControlKind.DONE})


if __name__ == "__main__":
    main()
