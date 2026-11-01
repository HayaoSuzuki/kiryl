# Copyright (c) 2026 kiryl contributors
"""Bounded, versioned JSON frames; target output never uses this stream."""

import json
import struct
from typing import TYPE_CHECKING, cast

if TYPE_CHECKING:
    from typing import BinaryIO

MAX_FRAME = 8 * 1024 * 1024


def _exact(stream: BinaryIO, size: int) -> bytes:
    chunks = bytearray()
    while len(chunks) < size:
        chunk = stream.read(size - len(chunks))
        if not chunk:
            message = "truncated control frame"
            raise EOFError(message)
        chunks.extend(chunk)
    return bytes(chunks)


def receive(stream: BinaryIO) -> dict[str, object]:
    """Read one bounded object and reject incompatible schema versions."""
    size = struct.unpack(">I", _exact(stream, 4))[0]
    if size > MAX_FRAME:
        message = "control frame exceeds limit"
        raise ValueError(message)
    value = json.loads(_exact(stream, size))
    if (
        not isinstance(value, dict)
        or type(value.get("version")) is not int
        or value["version"] != 1
    ):
        message = "incompatible control frame"
        raise ValueError(message)
    return cast("dict[str, object]", value)


def send(stream: BinaryIO, value: dict[str, object]) -> None:
    """Publish one complete frame or fail before the target continues."""
    encoded = json.dumps({**value, "version": 1}).encode()
    if len(encoded) > MAX_FRAME:
        message = "control frame exceeds limit"
        raise ValueError(message)
    stream.write(struct.pack(">I", len(encoded)) + encoded)
    stream.flush()
