# Copyright (c) 2026 kiryl contributors
"""Run finite fuzz invocations through pytest's real setup/teardown protocol."""

import json
import subprocess
import sys
import tempfile
import time
import unittest
from dataclasses import asdict, dataclass, field
from functools import partial
from pathlib import Path
from typing import TYPE_CHECKING, Protocol, override

from model import PytestPhase, ResultKind, Verification

if TYPE_CHECKING:
    from collections.abc import Callable, Generator

import pytest
from hypothesis import is_hypothesis_test

from hypothesis_bridge import (
    Evaluation,
    ShrinkEvidence,
    bind_target,
    instrument_ordinary,
    unpack_token,
)
from monitor import BranchMonitor


@dataclass
class Invocation:
    """One finite test invocation; function fixtures reset between these."""

    nodeid: str
    buffers: tuple[bytes, ...]


@dataclass
class CaseResult:
    """A case outcome with optional opaque reproduction data."""

    nodeid: str
    kind: ResultKind
    canonical: bytes | None = None
    fingerprint: str | None = None
    replay_token: bytes | None = None
    rendered: str | None = None
    verification: Verification = Verification.PENDING
    database_replay: bool = False
    features: tuple[str, ...] = ()
    elapsed_ns: int = 0


@dataclass
class SessionEvidence:
    """Collection, execution, and fixture observations from a worker session."""

    selected: list[str] = field(default_factory=list)
    cases: list[CaseResult] = field(default_factory=list)
    fixture_events: list[tuple[str, PytestPhase]] = field(default_factory=list)
    coverage_roots: tuple[str, ...] = ()


class CaseDriver(Protocol):
    """Exchange candidates and feedback without exposing Hypothesis internals."""

    def next_case(self, nodeid: str) -> bytes | None:
        """Return a candidate, or end the current invocation with None."""
        ...

    def observe(self, result: CaseResult) -> None:
        """Consume a case outcome before the next candidate is requested."""
        ...


MAX_RETAINED_CASES = 256

FIXTURE_EVENTS = pytest.StashKey[list[tuple[str, PytestPhase]]]()


class _InvocationItem(pytest.Function):
    owner: _Session
    invocation: Invocation

    @override
    def runtest(self) -> None:
        self.owner.execute(self, self.invocation)


class _Session:
    def __init__(
        self,
        invocations: list[Invocation],
        driver: CaseDriver | None,
        operation: Callable[[pytest.Function], None] | None = None,
        *,
        collect_coverage: bool = True,
    ) -> None:
        self.invocations = invocations
        self.driver = driver
        self.evidence = SessionEvidence()
        self.error: Exception | None = None
        self.operation = operation
        self._test_files: tuple[Path, ...] = ()
        self.collect_coverage = collect_coverage

    def pytest_configure(self, config: pytest.Config) -> None:
        config.stash[FIXTURE_EVENTS] = self.evidence.fixture_events

    @pytest.hookimpl(tryfirst=True)
    def pytest_runtestloop(self, session: pytest.Session) -> bool:
        if self.error:
            return True
        items = {
            item.nodeid: item
            for item in session.items
            if isinstance(item, pytest.Function) and is_hypothesis_test(item.obj)
        }
        self.evidence.selected = list(items)
        if self.collect_coverage:
            self.evidence.coverage_roots = (str(session.config.rootpath.resolve()),)
        self._test_files = tuple(item.path for item in session.items)
        if not items:
            self.error = ValueError("selection contains no supported Hypothesis tests")
            return True
        scheduled = []
        for invocation in self.invocations:
            original = items.get(invocation.nodeid)
            if original is None:
                self.error = ValueError(f"unknown selected Hypothesis test: {invocation.nodeid}")
                return True
            # Distinct invocation nodes let pytest tear down function fixtures
            # while retaining their shared module/session parents normally.
            clone = _InvocationItem.from_parent(
                original.parent,
                name=original.name,
                callspec=getattr(original, "callspec", None),
                fixtureinfo=original._fixtureinfo,  # noqa: SLF001 -- resolved parameter fixtures
                originalname=original.originalname,
            )
            clone.own_markers = list(original.own_markers)
            clone.owner = self
            clone.invocation = invocation
            scheduled.append(clone)
        for index, item in enumerate(scheduled):
            nextitem = scheduled[index + 1] if index + 1 < len(scheduled) else None
            session.config.hook.pytest_runtest_protocol(item=item, nextitem=nextitem)
        return True

    def pytest_collectreport(self, report: pytest.CollectReport) -> None:
        if report.failed:
            self.error = RuntimeError(f"pytest collection failed: {report.longrepr}")

    def execute(self, item: pytest.Function, invocation: Invocation) -> None:
        case = getattr(item.obj, "__self__", None)
        if isinstance(case, unittest.TestCase):
            self._unittest_context(case, partial(self._execute_inputs, item, invocation))
        else:
            self._execute_inputs(item, invocation)

    def _unittest_context(self, case: unittest.TestCase, call: Callable[[], None]) -> None:
        method = case.id().rsplit(".", 1)[-1]
        owned = method in vars(case)
        previous = vars(case).get(method)
        setattr(case, method, call)
        result = unittest.TestResult()
        try:
            case.run(result)
        finally:
            if owned:
                setattr(case, method, previous)
            else:
                delattr(case, method)
        if result.skipped:
            pytest.skip(result.skipped[0][1])
        if not result.wasSuccessful():
            message = f"unittest lifecycle failed: {result.errors!r} {result.failures!r}"
            raise RuntimeError(message)

    def _execute_inputs(self, item: pytest.Function, invocation: Invocation) -> None:
        if self.operation:
            self.operation(item)
            return
        target = bind_target(item)
        fixed = iter(invocation.buffers)
        monitor = (
            BranchMonitor((item.config.rootpath,), excluded=self._test_files)
            if self.collect_coverage
            else None
        )
        try:
            while True:
                raw = self.driver.next_case(item.nodeid) if self.driver else next(fixed, None)
                if raw is None:
                    return
                started = time.perf_counter_ns()
                if monitor:
                    monitor.start_case()
                try:
                    evaluated = target.check(raw)
                finally:
                    features = monitor.finish_case() if monitor else ()
                case = CaseResult(
                    item.nodeid,
                    **vars(evaluated),
                    features=features,
                    elapsed_ns=time.perf_counter_ns() - started,
                )
                if (
                    self.driver is None
                    or len(self.evidence.cases) < MAX_RETAINED_CASES
                    or case.kind == ResultKind.FAILURE
                ):
                    self.evidence.cases.append(case)
                if self.driver:
                    self.driver.observe(case)
                if case.kind in {ResultKind.FAILURE, ResultKind.SKIP}:
                    return
        finally:
            if monitor:
                monitor.close()

    def pytest_runtest_logreport(self, report: pytest.TestReport) -> None:
        if report.skipped:
            case = CaseResult(report.nodeid, ResultKind.SKIP)
            self.evidence.cases.append(case)
            if self.driver:
                self.driver.observe(case)
        if report.failed:
            self.evidence.cases.append(
                CaseResult(
                    report.nodeid, ResultKind.INFRASTRUCTURE, rendered=str(report.longrepr)[-4096:]
                )
            )


def run_session(
    pytest_args: list[str],
    invocations: list[Invocation],
    driver: CaseDriver | None = None,
    *,
    collect_coverage: bool = True,
) -> SessionEvidence:
    """Collect and execute finite invocations inside one real pytest session."""
    plugin = _Session(invocations, driver, collect_coverage=collect_coverage)
    result = pytest.main(pytest_args, plugins=[plugin])
    if plugin.error:
        raise plugin.error
    if result not in (pytest.ExitCode.OK, pytest.ExitCode.TESTS_FAILED):
        message = f"pytest infrastructure exit: {result}"
        raise RuntimeError(message)
    return plugin.evidence


def _in_context(
    nodeid: str, pytest_args: list[str], operation: Callable[[pytest.Function], None]
) -> SessionEvidence:
    plugin = _Session([Invocation(nodeid, ())], None, operation)
    pytest.main(pytest_args, plugins=[plugin])
    if plugin.error:
        raise plugin.error
    if any(case.kind != ResultKind.SKIP for case in plugin.evidence.cases):
        message = plugin.evidence.cases[0].rendered or "operation infrastructure failure"
        raise RuntimeError(message)
    return plugin.evidence


def replay(nodeid: str, token: bytes, pytest_args: list[str]) -> CaseResult:
    """Verify a stored case in a separate, fresh Python interpreter."""
    unpack_token(token)
    with tempfile.TemporaryDirectory() as directory:
        request = Path(directory) / "request.json"
        output = Path(directory) / "result.json"
        request.write_text(
            json.dumps({"nodeid": nodeid, "token": token.hex(), "pytest_args": pytest_args})
        )
        completed = subprocess.run(  # noqa: S603 -- owned interpreter/bridge and trusted local tests
            [sys.executable, __file__, str(request), str(output)],
            capture_output=True,
            timeout=30,
            check=False,
        )
        if completed.returncode or not output.exists():
            return CaseResult(
                nodeid,
                ResultKind.INFRASTRUCTURE,
                rendered=completed.stderr.decode(errors="replace")[-4096:],
            )
        value = json.loads(output.read_text())
        for key in ("canonical", "replay_token"):
            if value.get(key) is not None:
                value[key] = bytes.fromhex(value[key])
        value["features"] = tuple(value.get("features", ()))
        value["kind"] = ResultKind(value["kind"])
        value["verification"] = Verification(value["verification"])
        return CaseResult(**value)


def minimize(
    nodeid: str, token: bytes, budget_seconds: float, before_case: Callable[[bytes], None]
) -> ShrinkEvidence:
    """Reduce a finding in its pytest context; token owns recorded invocation args."""
    value = unpack_token(token)
    result = None

    def operation(item: pytest.Function) -> None:
        nonlocal result
        result = bind_target(item).minimize(token, budget_seconds, before_case)

    evidence = _in_context(nodeid, value["pytest_args"], operation)
    if result is None and any(case.kind == ResultKind.SKIP for case in evidence.cases):
        pytest.skip("minimization invocation skipped")
    if result is None:
        message = "minimization did not execute a target"
        raise RuntimeError(message)
    return result


def run_token(
    nodeid: str,
    token: bytes,
    pytest_args: list[str],
    before_case: Callable[[], None] | None = None,
) -> CaseResult:
    """Evaluate an opaque replay token inside this worker's fresh pytest context."""
    result = None

    def operation(item: pytest.Function) -> None:
        nonlocal result
        target = bind_target(item)
        if before_case:
            before_case()
        evaluated = target.check_token(token)
        expected = unpack_token(token).get("fingerprint")
        status = (
            Verification.CONFIRMED
            if evaluated.kind == ResultKind.FAILURE and evaluated.fingerprint == expected
            else Verification.UNSTABLE
        )
        result = CaseResult(item.nodeid, **vars(evaluated), verification=status)

    evidence = _in_context(nodeid, pytest_args, operation)
    if result is None and any(case.kind == ResultKind.SKIP for case in evidence.cases):
        return CaseResult(nodeid, ResultKind.SKIP, verification=Verification.UNSTABLE)
    if result is None:
        message = "replay did not execute a target"
        raise RuntimeError(message)
    return result


def _replay_main() -> None:
    request = json.loads(Path(sys.argv[1]).read_text())
    result = run_token(request["nodeid"], bytes.fromhex(request["token"]), request["pytest_args"])
    value = asdict(result)
    for key in ("canonical", "replay_token"):
        if value[key] is not None:
            value[key] = value[key].hex()
    Path(sys.argv[2]).write_text(json.dumps(value))


if __name__ == "__main__":
    # Fixtures importing the adapter must see the same stash key/class objects
    # as the bootstrap, rather than a second import under a different name.
    sys.modules["adapter"] = sys.modules[__name__]
    _replay_main()


class _OrdinarySession:
    """Measure the normal pytest/Hypothesis path without replacing its run loop."""

    def __init__(self, before: Callable[[str], None], after: Callable[[CaseResult], None]) -> None:
        self.before = before
        self.after = after
        self.excluded: tuple[Path, ...] = ()
        self.items: list[pytest.Item] = []

    def pytest_collection_modifyitems(self, items: list[pytest.Item]) -> None:
        self.items = items
        self.excluded = tuple(item.path for item in items)

    def pytest_runtest_logreport(self, report: pytest.TestReport) -> None:
        if report.skipped and PytestPhase(report.when) == PytestPhase.SETUP:
            self.after(CaseResult(report.nodeid, ResultKind.SKIP))

    @pytest.hookimpl(wrapper=True, trylast=True)
    def pytest_runtest_call(self, item: pytest.Item) -> Generator[None]:
        if not isinstance(item, pytest.Function) or not is_hypothesis_test(item.obj):
            yield
            return
        monitor = BranchMonitor((item.config.rootpath,), excluded=self.excluded)
        started = 0

        def before() -> None:
            nonlocal started
            self.before(item.nodeid)
            started = time.perf_counter_ns()
            monitor.start_case()

        def after(evaluated: Evaluation) -> None:
            features = monitor.finish_case()
            self.after(
                CaseResult(
                    item.nodeid,
                    **vars(evaluated),
                    features=features,
                    elapsed_ns=time.perf_counter_ns() - started,
                )
            )

        restore = instrument_ordinary(item, before, after)
        try:
            yield
        finally:
            restore()
            monitor.close()


def run_ordinary(
    pytest_args: list[str], before: Callable[[str], None], after: Callable[[CaseResult], None]
) -> tuple[int, list[str]]:
    """Run unchanged Hypothesis generation/shrinking with measurement callbacks."""
    plugin = _OrdinarySession(before, after)
    code = pytest.main(pytest_args, plugins=[plugin])
    statistics = [getattr(item, "hypothesis_statistics", "") for item in plugin.items]
    return int(code), [text for text in statistics if isinstance(text, str) and text]
