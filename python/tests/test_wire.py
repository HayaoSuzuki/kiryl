# Copyright (c) 2026 kiryl contributors
"""Protocol boundaries reject malformed frames before dispatch."""

import io
import json
import struct

import pytest
from wire import MAX_FRAME, receive, send


@pytest.mark.parametrize("payload", [b"", b"\x00", struct.pack(">I", 3) + b"{"])
def test_truncated_frame_is_rejected(payload: bytes) -> None:
    """Treat incomplete headers and bodies as transport failures."""
    with pytest.raises(EOFError, match="truncated"):
        receive(io.BytesIO(payload))


def test_oversized_incoming_frame_is_rejected() -> None:
    """Reject a declared length before attempting to allocate its payload."""
    with pytest.raises(ValueError, match="exceeds limit"):
        receive(io.BytesIO(struct.pack(">I", MAX_FRAME + 1)))


@pytest.mark.parametrize("value", [[], {}, {"version": True}, {"version": 1.0}, {"version": 2}])
def test_incompatible_frame_schema_is_rejected(value: object) -> None:
    """Require an object with an exact integer protocol version."""
    encoded = json.dumps(value).encode()
    with pytest.raises(ValueError, match="incompatible"):
        receive(io.BytesIO(struct.pack(">I", len(encoded)) + encoded))


def test_oversized_outgoing_frame_writes_nothing() -> None:
    """A rejected publication must not leave a partial control frame."""
    output = io.BytesIO()
    with pytest.raises(ValueError, match="exceeds limit"):
        send(output, {"payload": "x" * MAX_FRAME})
    assert output.getvalue() == b""


def test_frame_round_trip_preserves_payload() -> None:
    """Publish complete bytes and supply the protocol version centrally."""
    output = io.BytesIO()
    send(output, {"kind": "ack", "version": 999})
    output.seek(0)
    assert receive(output) == {"kind": "ack", "version": 1}
