# Copyright (c) 2026 kiryl contributors
"""Domain states shared by the adapter and its serialization boundaries."""

from dataclasses import dataclass
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


class BenchmarkMode(StrEnum):
    """Discovery comparison algorithms with separate storage namespaces."""

    GUIDED = "guided"
    UNGUIDED = "unguided"
    HYPOTHESIS = "hypothesis"


class ComparisonTarget(StrEnum):
    """Authored target families in the bounded comparison."""

    BRANCHING = "branching"
    STATEFUL = "stateful"


class ProbeStatus(StrEnum):
    """Only observed evidence can advance an unverified gate."""

    PASSED = "passed"
    FAILED = "failed"
    UNVERIFIED = "unverified"


class SummaryStatus(StrEnum):
    """Aggregate evidence remains partial until all gates pass."""

    PASSED = "passed"
    FAILED = "failed"
    PARTIAL = "partial"


class ResourceBackend(StrEnum):
    """Native capability labels decoded from controller evidence."""

    PORTABLE = "portable"
    CGROUP_V2 = "cgroup-v2"
    JOB_OBJECT = "job-object"


class OutcomeKind(StrEnum):
    """Controller outcomes include execution and process supervision states."""

    PASS = "pass"  # noqa: S105 -- execution outcome, not a credential
    REJECT = "reject"
    FAILURE = "failure"
    SKIP = "skip"
    INFRASTRUCTURE = "infrastructure"
    NATIVE_CRASH = "native_crash"
    CASE_TIMEOUT = "case_timeout"
    CAMPAIGN_CANCELLED = "campaign_cancelled"
    INTERRUPTED = "interrupted"
    OOM = "oom"


@dataclass(frozen=True)
class VerificationDiagnostic:
    """External reports may carry a free diagnostic instead of a known status."""

    text: str


def decode_verification(value: object) -> Verification | VerificationDiagnostic:
    """Preserve controller diagnostics while recognizing executable status labels."""
    if not isinstance(value, str):
        message = "verification must be a string"
        raise TypeError(message)
    try:
        return Verification(value)
    except ValueError:
        return VerificationDiagnostic(value)


class ArtifactMode(StrEnum):
    """Persisted evidence distinguishes semantic and original-byte replay."""

    REPLAY = "replay"
    RAW_REPLAY = "raw_replay"
    MINIMIZE = "minimize"


@dataclass(frozen=True)
class UnknownOutcome:
    """An extensible controller label retained without treating it as a failure."""

    text: str


def decode_outcome(value: object) -> OutcomeKind | UnknownOutcome:
    """Decode only string labels, preserving future classifications as opaque data."""
    if not isinstance(value, str):
        message = "outcome kind must be a string"
        raise TypeError(message)
    try:
        return OutcomeKind(value)
    except ValueError:
        return UnknownOutcome(value)
