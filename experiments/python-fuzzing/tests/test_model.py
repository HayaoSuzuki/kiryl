# Copyright (c) 2026 kiryl contributors
"""State values cross JSON boundaries without losing their domain types."""

import json
import sys
from dataclasses import asdict

import pytest

from adapter import CaseResult
from hypothesis_bridge import SUPPORTED_VERSION, unpack_token
from model import (
    ControlKind,
    OutcomeKind,
    ReplayKind,
    ResultKind,
    UnknownOutcome,
    Verification,
    WorkerMode,
    decode_outcome,
)
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


@pytest.mark.parametrize(
    "label", ["cleanup failed: unavailable", "hard memory backend unavailable", ""]
)
def test_diagnostic_verification_is_preserved(label: str) -> None:
    """Free diagnostic evidence cannot be misclassified as a known status."""
    from model import VerificationDiagnostic, decode_verification  # noqa: PLC0415 -- boundary contract under test

    value = decode_verification(label)
    assert value == VerificationDiagnostic(label)
    assert value != Verification.CONFIRMED


@pytest.mark.parametrize("label", [None, 1, {}])
def test_non_string_verification_is_rejected(label: object) -> None:
    """A malformed report cannot impersonate a typed verification state."""
    from model import decode_verification  # noqa: PLC0415 -- boundary contract under test

    with pytest.raises(TypeError, match="verification must be a string"):
        decode_verification(label)


def test_known_verification_decodes_to_enum() -> None:
    """A report's confirmed label becomes an actual enum member."""
    from model import decode_verification  # noqa: PLC0415 -- boundary contract under test

    assert decode_verification("confirmed") is Verification.CONFIRMED


@pytest.mark.parametrize("value", [None, 7, {}, []])
def test_outcome_boundary_rejects_non_string_classifications(value: object) -> None:
    """Malformed JSON values cannot become controller state labels."""
    with pytest.raises(TypeError, match="outcome kind must be a string"):
        decode_outcome(value)


@pytest.mark.parametrize(
    ("label", "expected"),
    [
        ("pass", OutcomeKind.PASS),
        ("failure", OutcomeKind.FAILURE),
        ("future-result", UnknownOutcome("future-result")),
        ("", UnknownOutcome("")),
    ],
)
def test_outcome_boundary_retains_known_or_future_identity(
    label: str, expected: OutcomeKind | UnknownOutcome
) -> None:
    """Extensible labels retain their exact value rather than collapsing states."""
    assert decode_outcome(label) == expected
