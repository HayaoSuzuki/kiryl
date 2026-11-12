# Copyright (c) 2026 kiryl contributors
"""Domain states shared by the adapter and its serialization boundaries."""

from enum import StrEnum


class ResultKind(StrEnum):
    """Closed vocabulary for ResultKind values; JSON retains existing labels."""

    PASS = "pass"  # noqa: S105 -- execution outcome, not a credential
    REJECT = "reject"
    FAILURE = "failure"
    SKIP = "skip"
    INFRASTRUCTURE = "infrastructure"


class Verification(StrEnum):
    """Closed vocabulary for Verification values; JSON retains existing labels."""

    PENDING = "pending"
    CONFIRMED = "confirmed"
    UNSTABLE = "unstable"


class ReplayKind(StrEnum):
    """Closed vocabulary for ReplayKind values; JSON retains existing labels."""

    RAW = "raw"
    CHOICES = "choices"


class WorkerMode(StrEnum):
    """Closed vocabulary for WorkerMode values; JSON retains existing labels."""

    RUN = "run"
    BASELINE = "baseline"
    REPLAY = "replay"
    MINIMIZE = "minimize"


class ControlKind(StrEnum):
    """Closed vocabulary for ControlKind values; JSON retains existing labels."""

    NEXT = "next"
    END = "end"
    CASE = "case"
    BASELINE_CASE = "baseline_case"
    ACK = "ack"
    SKIPPED = "skipped"
    RESULT = "result"
    READY = "ready"
    START = "start"
    ERROR = "error"
    BASELINE_DONE = "baseline_done"
    REPLAY_CASE = "replay_case"
    CANDIDATE = "candidate"
    SHRUNK = "shrunk"
    DONE = "done"


class PytestPhase(StrEnum):
    """Closed vocabulary for PytestPhase values; JSON retains existing labels."""

    SETUP = "setup"
    CALL = "call"
    TEARDOWN = "teardown"
