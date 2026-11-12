# Copyright (c) 2026 kiryl contributors
"""State values cross JSON boundaries without losing their domain types."""

import json
import sys
from dataclasses import asdict

import pytest
from adapter import CaseResult
from hypothesis_bridge import SUPPORTED_VERSION, unpack_token
from model import ControlKind, ReplayKind, ResultKind, Verification, WorkerMode
from worker import _control_kind, _operation_mode


def test_case_state_json_encoding() -> None:
    """Stored result labels retain their exact public spelling."""
    result = CaseResult("target", ResultKind.FAILURE, verification=Verification.CONFIRMED)
    value = json.loads(json.dumps(asdict(result)))
    assert value["kind"] == "failure"
    assert value["verification"] == "confirmed"


@pytest.mark.parametrize("label", ["unknown", "", "PASS"])
def test_unknown_result_state_is_rejected(label: str) -> None:
    """A malformed external state cannot become an executable internal state."""
    with pytest.raises(ValueError, match="not a valid ResultKind"):
        ResultKind(label)


@pytest.mark.parametrize(
    ("label", "expected"), [("case", ControlKind.CASE), ("ack", ControlKind.ACK), ("unknown", None)]
)
def test_controller_label_decoding(label: str, expected: ControlKind | None) -> None:
    """Unknown controller labels remain rejectable by the active operation."""
    assert _control_kind({"kind": label}) is expected


@pytest.mark.parametrize(
    ("label", "expected"), [("run", WorkerMode.RUN), ("minimize", WorkerMode.MINIMIZE)]
)
def test_operation_label_decoding(label: str, expected: WorkerMode) -> None:
    """External mode labels become typed operation choices."""
    assert _operation_mode(label) is expected


@pytest.mark.parametrize("label", ["unknown", "", None, 42])
def test_invalid_operation_keeps_diagnostic(label: object) -> None:
    """Invalid operation labels never select a default execution path."""
    with pytest.raises(ValueError, match="unknown worker mode"):
        _operation_mode(label)


@pytest.mark.parametrize(
    ("label", "expected"), [("raw", ReplayKind.RAW), ("choices", ReplayKind.CHOICES)]
)
def test_replay_kind_decoding(label: str, expected: ReplayKind) -> None:
    """Replay payload formats decode into their own domain."""
    token = json.dumps(
        {
            "version": 1,
            "hypothesis": SUPPORTED_VERSION,
            "python": list(sys.version_info[:2]),
            "nodeid": "target",
            "pytest_args": [],
            "kind": label,
            "payload": "",
        }
    ).encode()
    assert unpack_token(token)["kind"] is expected
