# Copyright (c) 2026 kiryl contributors
"""Run finite fuzz invocations through pytest's real setup/teardown protocol."""

from dataclasses import dataclass, field
from typing import Protocol, override

import pytest
from hypothesis import is_hypothesis_test

from hypothesis_bridge import bind_target


@dataclass
class Invocation:
    """One finite test invocation; function fixtures reset between these."""

    nodeid: str
    buffers: tuple[bytes, ...]


@dataclass
class CaseResult:
    """A case outcome with optional opaque reproduction data."""

    nodeid: str
    kind: str
    canonical: bytes | None = None
    fingerprint: str | None = None
    replay_token: bytes | None = None
    rendered: str | None = None


@dataclass
class SessionEvidence:
    """Collection, execution, and fixture observations from a worker session."""

    selected: list[str] = field(default_factory=list)
    cases: list[CaseResult] = field(default_factory=list)
    fixture_events: list[tuple[str, str]] = field(default_factory=list)


class CaseDriver(Protocol):
    """Exchange candidates and feedback without exposing Hypothesis internals."""

    def next_case(self, nodeid: str) -> bytes | None:
        """Return a candidate, or end the current invocation with None."""
        ...

    def observe(self, result: CaseResult) -> None:
        """Consume a case outcome before the next candidate is requested."""
        ...


FIXTURE_EVENTS = pytest.StashKey[list[tuple[str, str]]]()


class _InvocationItem(pytest.Function):
    owner: _Session
    invocation: Invocation

    @override
    def runtest(self) -> None:
        self.owner.execute(self, self.invocation)


class _Session:
    def __init__(self, invocations: list[Invocation], driver: CaseDriver | None) -> None:
        self.invocations = invocations
        self.driver = driver
        self.evidence = SessionEvidence()
        self.error: Exception | None = None

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
        target = bind_target(item)
        fixed = iter(invocation.buffers)
        while True:
            raw = self.driver.next_case(item.nodeid) if self.driver else next(fixed, None)
            if raw is None:
                return
            evaluated = target.check(raw)
            case = CaseResult(item.nodeid, **vars(evaluated))
            self.evidence.cases.append(case)
            if self.driver:
                self.driver.observe(case)
            if case.kind == "failure":
                return

    def pytest_runtest_logreport(self, report: pytest.TestReport) -> None:
        if report.failed:
            self.evidence.cases.append(
                CaseResult(report.nodeid, "infrastructure", rendered=str(report.longrepr)[-4096:])
            )


def run_session(
    pytest_args: list[str], invocations: list[Invocation], driver: CaseDriver | None = None
) -> SessionEvidence:
    """Collect and execute finite invocations inside one real pytest session."""
    plugin = _Session(invocations, driver)
    result = pytest.main(pytest_args, plugins=[plugin])
    if plugin.error:
        raise plugin.error
    if result not in (pytest.ExitCode.OK, pytest.ExitCode.TESTS_FAILED):
        message = f"pytest infrastructure exit: {result}"
        raise RuntimeError(message)
    return plugin.evidence
