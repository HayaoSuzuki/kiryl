# Copyright (c) 2026 kiryl contributors
"""Report presentation failures remain visible after an acknowledged finding."""

import json
import subprocess
import sys
from typing import TYPE_CHECKING

import pytest
from test_cli import BINARY

if TYPE_CHECKING:
    from pathlib import Path

pytestmark = [
    pytest.mark.skipif(
        sys.platform == "win32", reason="owned executable POSIX interpreter wrapper"
    ),
    pytest.mark.skipif(not BINARY.exists(), reason="cargo build required"),
]


@pytest.fixture
def corrupting_interpreter(tmp_path: Path) -> Path:
    """Emulate an interpreter and corrupt only its acknowledged temporary finding."""
    original = tmp_path / "store" / "target-0" / "finding-0.bin"
    wrapper = tmp_path / "python-probe"
    wrapper.write_text(
        f"#!{sys.executable}\n"
        "import json, struct, sys\nfrom pathlib import Path\n"
        f"original = Path({str(original)!r})\n"
        """
def receive():
    size = struct.unpack(">I", sys.stdin.buffer.read(4))[0]
    return json.loads(sys.stdin.buffer.read(size))

def send(value):
    data = json.dumps({**value, "version": 1}).encode()
    sys.stdout.buffer.write(struct.pack(">I", len(data)) + data)
    sys.stdout.buffer.flush()

if "-u" not in sys.argv:
    assert sys.stdin.buffer.read(1) == b"g"
    print(json.dumps({"nodeids": ["test"], "environment": {}, "sources": {},
                      "implementation_sources": {}}))
else:
    send({"kind": "ready", "environment": {}})
    assert receive()["kind"] == "start"
    send({"kind": "next", "test_id": "test"})
    case = receive()
    assert case["kind"] == "case"
    send({"kind": "result", "case_id": case["case_id"], "observation": {
        "kind": "failure", "nodeid": "test", "fingerprint": "mock", "replay_token": None,
    }})
    assert receive()["kind"] == "ack"
    saved = json.loads(original.read_text())
    assert saved["test_id"] == "test"
    original.write_text("{corrupt-owned-finding")
    send({"kind": "next", "test_id": "test"})
    assert receive()["kind"] == "end"
    send({"kind": "done"})
""",
    )
    wrapper.chmod(0o700)
    return wrapper


def test_corrupt_finding_presentation_replaces_stale_success(
    tmp_path: Path,
    corrupting_interpreter: Path,
) -> None:
    """A corruption after persistence acknowledgment fails the CLI's presentation stage."""
    destination = tmp_path / "report.json"
    destination.write_text('{"status":"success","stale":true}')
    result = subprocess.run(  # noqa: S603 -- built CLI and owned fault-injection interpreter
        [
            str(BINARY),
            "run",
            "test",
            "--best-effort",
            "--duration",
            "2",
            "--store",
            str(tmp_path / "store"),
            "--report",
            str(destination),
            "--python",
            str(corrupting_interpreter),
        ],
        cwd=tmp_path,
        capture_output=True,
        check=False,
        timeout=20,
    )
    assert result.returncode == 2, result.stderr  # noqa: PLR2004 -- infrastructure exit
    report = json.loads(destination.read_text())
    assert report["status"] == "infrastructure"
    assert report["selection_complete"] is True
    assert report["infrastructure_errors"]
    assert "stale" not in report
    assert (
        tmp_path / "store" / "target-0" / "finding-0.bin"
    ).read_text() == "{corrupt-owned-finding"
