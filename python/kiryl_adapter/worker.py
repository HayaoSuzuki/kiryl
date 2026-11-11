# Copyright (c) 2026 kiryl contributors
"""Trusted product bootstrap; controller owns candidates and all persistence."""

import os
import sys
from dataclasses import asdict
from typing import TYPE_CHECKING, cast

from wire import receive, send

if TYPE_CHECKING:
    from typing import BinaryIO

    from adapter import CaseResult


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
        send(self.control, {"kind": "next", "test_id": nodeid})
        message = receive(self.input)
        if message["kind"] == "end":
            return None
        if message["kind"] != "case":
            error = "unexpected controller response"
            raise ValueError(error)
        self.case_id = cast("int", message["case_id"])
        return bytes(cast("list[int]", message["input"]))

    def begin_ordinary(self, nodeid: str) -> None:
        """Publish normal Hypothesis execution boundaries without requesting input."""
        self.case_id += 1
        send(self.control, {"kind": "baseline_case", "test_id": nodeid, "case_id": self.case_id})
        if receive(self.input)["kind"] != "ack":
            message = "missing ordinary-case acknowledgment"
            raise ValueError(message)

    def observe(self, result: CaseResult) -> None:
        """Wait until the parent has saved any original before continuing."""
        send(
            self.control,
            {
                "kind": "skipped" if result.kind == "skip" else "result",
                "case_id": self.case_id,
                "observation": _observation(result),
            },
        )
        if receive(self.input)["kind"] != "ack":
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
            "kind": "ready",
            "environment": {
                "platform": sys.platform,
                "python": list(sys.version_info[:2]),
                "implementation": sys.implementation.name,
            },
        },
    )
    request = receive(sys.stdin.buffer)
    if request["kind"] != "start":
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


def _dispatch(control: BinaryIO, request: dict[str, object]) -> None:
    """Run one attached operation."""
    from adapter import Invocation, minimize, run_ordinary, run_session, run_token  # noqa: PLC0415 -- after attachment

    nodeid = cast("str", request["test_id"])
    args = cast("list[str]", request["pytest_args"])
    mode = request.get("mode", "run")
    driver = Driver(control)
    if mode == "run":
        evidence = run_session(args, [Invocation(nodeid, ())], driver)
        infrastructure = [asdict(case) for case in evidence.cases if case.kind == "infrastructure"]
        if infrastructure:
            send(control, {"kind": "error", "message": str(infrastructure)[-4096:]})
    elif mode == "baseline":
        code, statistics = run_ordinary(args, driver.begin_ordinary, driver.observe)
        send(control, {"kind": "baseline_done", "exit_code": code, "statistics": statistics})
    elif mode == "replay":
        token = bytes(cast("list[int]", request["token"]))

        def before_replay() -> None:
            send(control, {"kind": "replay_case", "test_id": nodeid})
            if receive(driver.input)["kind"] != "ack":
                message = "missing replay-case acknowledgment"
                raise ValueError(message)

        driver.observe(run_token(nodeid, token, args, before_replay))
    elif mode == "minimize":
        token = bytes(cast("list[int]", request["token"]))

        def before_case(candidate: bytes) -> None:
            send(control, {"kind": "candidate", "token": list(candidate)})
            if receive(driver.input)["kind"] != "ack":
                message = "missing shrink candidate acknowledgment"
                raise ValueError(message)

        shrunk = minimize(nodeid, token, cast("float", request["budget_seconds"]), before_case)
        send(control, {"kind": "shrunk", "token": list(shrunk.best), "complete": shrunk.complete})
    else:
        message = "unknown worker mode"
        raise ValueError(message)
    send(control, {"kind": "done"})


def main() -> None:
    """Dispatch an attached worker operation without exposing target stdout."""
    control, request = _bootstrap()
    from adapter import CaseResult  # noqa: PLC0415 -- after attachment
    from hypothesis.core import skip_exceptions_to_reraise  # noqa: PLC0415 -- after attachment

    try:
        import hypothesis  # noqa: PLC0415 -- after resource attachment
        import pytest  # noqa: PLC0415 -- after resource attachment

        if (
            sys.implementation.name != "cpython"
            or sys.version_info[:2] not in ((3, 14), (3, 15))
            or not sys._is_gil_enabled()  # noqa: SLF001 -- CPython runtime capability
            or hypothesis.__version__ != "6.170.13"
            or pytest.__version__ != "9.2.0"
        ):
            send(
                control,
                {"kind": "error", "message": "unsupported Python/pytest/Hypothesis runtime"},
            )
            return
        _dispatch(control, request)
    except skip_exceptions_to_reraise():
        Driver(control).observe(CaseResult(cast("str", request["test_id"]), "skip"))
        send(control, {"kind": "done"})


if __name__ == "__main__":
    main()
