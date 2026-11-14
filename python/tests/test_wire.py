# Copyright (c) 2026 kiryl contributors
"""Protocol boundaries reject malformed frames before dispatch."""

import io
import json
import struct
from typing import cast, override

import pytest
from wire import MAX_FRAME, receive, send


class FiniteControlStream(io.BytesIO):
    """Fail boundedly if the controller keeps reading after an observed EOF."""

    def __init__(self, payload: bytes) -> None:
        """Own one finite control exchange."""
        super().__init__(payload)
        self._eof_seen = False

    @override
    def read(self, size: int | None = -1, /) -> bytes:
        """Permit the first EOF while rejecting a busy loop on an exhausted stream."""
        value = super().read(size)
        assert value or not self._eof_seen, "control reader continued after EOF"
        self._eof_seen = not value
        return value


@pytest.fixture(params=[8 * 1024 * 1024 - 1, 8 * 1024 * 1024], ids=["below-limit", "at-limit"])
def boundary_frame(request: pytest.FixtureRequest) -> tuple[dict[str, object], bytes]:
    """Build a frame against the documented limit independently of MAX_FRAME."""
    size = cast("int", request.param)
    value: dict[str, object] = {"version": 1, "payload": ""}
    value["payload"] = "x" * (size - len(json.dumps(value).encode()))
    encoded = json.dumps(value).encode()
    assert len(encoded) == size
    return value, encoded


@pytest.mark.parametrize("payload", [b"", b"\x00", struct.pack(">I", 3) + b"{"])
def test_truncated_frame_is_rejected(payload: bytes) -> None:
    """Treat incomplete headers and bodies as transport failures."""
    with pytest.raises(EOFError, match="truncated"):
        receive(FiniteControlStream(payload))


def test_incoming_frame_accepts_documented_size_boundary(
    boundary_frame: tuple[dict[str, object], bytes],
) -> None:
    """Receive every legal byte at the documented eight MiB boundary."""
    value, encoded = boundary_frame
    assert receive(io.BytesIO(struct.pack(">I", len(encoded)) + encoded)) == value


def test_outgoing_frame_accepts_documented_size_boundary(
    boundary_frame: tuple[dict[str, object], bytes],
) -> None:
    """Publish a complete legal frame without an off-by-one size rejection."""
    value, encoded = boundary_frame
    output = io.BytesIO()
    send(output, value)
    assert output.getvalue() == struct.pack(">I", len(encoded)) + encoded


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
