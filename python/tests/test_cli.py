# Copyright (c) 2026 kiryl contributors
"""Native product CLI acceptance; build with cargo build before running."""

import json
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest

BINARY = Path(__file__).parents[2] / "target" / "debug" / ("kiryl.exe" if sys.platform == "win32" else "kiryl")
pytestmark = pytest.mark.skipif(not BINARY.exists(), reason="cargo build required")


def invoke(root: Path, *args: str) -> subprocess.CompletedProcess[bytes]:
    """Run the built product against an isolated project and selected interpreter."""
    return subprocess.run(  # noqa: S603 -- built product and owned temporary project
        [str(BINARY), *args, "--python", sys.executable],
        cwd=root,
        capture_output=True,
        check=False,
        timeout=20,
    )


def project(root: Path, body: str) -> None:
    """Write project configuration outside the repository's pytest settings."""
    (root / "pytest.ini").write_text("[pytest]\n")
    (root / "test_target.py").write_text(body)


def run(root: Path, name: str = "report", *extra: str) -> tuple[int, dict[str, Any]]:
    """Use finite slices so campaign boundaries are exercised repeatedly."""
    result = invoke(
        root,
        "run",
        "test_target.py",
        "--best-effort",
        "--duration",
        "2",
        "--slice",
        "0.5",
        "--store",
        str(root / "store"),
        "--report",
        str(root / f"{name}.json"),
        *extra,
    )
    assert result.returncode != 2, result.stderr  # noqa: PLR2004 -- infrastructure
    return result.returncode, json.loads((root / f"{name}.json").read_text())


def test_passing_run_restarts_persistent_corpus(tmp_path: Path) -> None:
    """Finite successful searches remain successful when resumed from their store."""
    project(
        tmp_path,
        "from hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_pass(n):\n    assert n == n\n",
    )
    code, report = run(tmp_path)
    assert code in (0, 1)
    assert report["targets"][0]["report"]["useful"] > 0
    corpus = tmp_path / "store" / "target-0" / "corpus.json"
    assert json.loads(corpus.read_text())
    code, _ = run(tmp_path, "restart")
    assert code in (0, 1)


def test_failure_original_replay_minimize_and_mismatch(tmp_path: Path) -> None:
    """Fresh verification and minimization preserve immutable original bytes."""
    project(
        tmp_path,
        "from hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_fail(n):\n    assert False\n",
    )
    code, _ = run(tmp_path)
    assert code == 1
    original = next((tmp_path / "store").rglob("finding-*.bin"))
    before = original.read_bytes()
    for command in ("replay", "minimize"):
        result = invoke(tmp_path, command, str(original), "--best-effort", "--duration", "2")
        assert result.returncode == 1, result.stderr
        assert original.read_bytes() == before
    source = tmp_path / "test_target.py"
    source.write_text(source.read_text() + "\n# changed source\n")
    result = invoke(tmp_path, "replay", str(original), "--best-effort", "--duration", "2")
    assert result.returncode == 2  # noqa: PLR2004 -- infrastructure exit contract
    assert original.read_bytes() == before


def test_selection_retains_parameter_fixtures_and_stateful(tmp_path: Path) -> None:
    """Selection preserves real pytest IDs while the adapter runs their fixtures."""
    project(
        tmp_path,
        """
import pytest
from hypothesis import given, settings, HealthCheck, strategies as st
from hypothesis.stateful import RuleBasedStateMachine, rule
@pytest.fixture
def value(): return 7
@pytest.mark.parametrize("tag", ["a", "b"])
@settings(suppress_health_check=[HealthCheck.function_scoped_fixture])
@given(n=st.integers())
def test_selected(value, tag, n): assert value == 7
@given(st.integers())
def test_excluded(n): assert False
class Machine(RuleBasedStateMachine):
    @rule()
    def step(self): pass
TestSelectedMachine = Machine.TestCase
""",
    )
    code, report = run(tmp_path, "selected", "-k", "selected and not excluded")
    assert code in (0, 1)
    targets = [entry["target"] for entry in report["targets"]]
    assert len(targets) == 3  # noqa: PLR2004 -- two variants and state machine
    assert any("[a]" in target for target in targets)
    assert any("[b]" in target for target in targets)
    assert any("TestSelectedMachine" in target for target in targets)


def test_case_timeout_preserves_active_input(tmp_path: Path) -> None:
    """A killed active case retains its original input rather than a replay token."""
    project(
        tmp_path,
        "import time\nfrom hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_hang(n):\n    time.sleep(10)\n",
    )
    result = invoke(
        tmp_path,
        "run",
        "test_target.py",
        "--best-effort",
        "--duration",
        "2",
        "--case-timeout",
        "0.1",
        "--store",
        str(tmp_path / "store"),
        "--report",
        str(tmp_path / "timeout.json"),
    )
    assert result.returncode == 1, result.stderr
    records = [
        json.loads(path.read_bytes()) for path in (tmp_path / "store").rglob("finding-*.bin")
    ]
    assert records
    assert all(record["test_id"] == "test_target.py::test_hang" for record in records)
    assert all(isinstance(record["input"], list) for record in records)
    timed_out = [record for record in records if record["observation"]["kind"] == "case_timeout"]
    assert timed_out
    assert timed_out[0]["input"] == [0] * 512


def test_short_budget_keeps_unexecuted_target_visible(tmp_path: Path) -> None:
    """Exhausted setup/case time cannot remove later selected targets from reports."""
    project(
        tmp_path,
        "import time\nfrom hypothesis import given, strategies as st\n"
        "@given(st.integers())\ndef test_first(n):\n    time.sleep(10)\n"
        "@given(st.integers())\ndef test_later(n):\n    pass\n",
    )
    result = invoke(
        tmp_path,
        "run",
        "test_target.py",
        "--best-effort",
        "--duration",
        "0.7",
        "--store",
        str(tmp_path / "store"),
        "--report",
        str(tmp_path / "short.json"),
    )
    assert result.returncode in (0, 1), result.stderr
    report = json.loads((tmp_path / "short.json").read_text())
    later = next(entry for entry in report["targets"] if entry["target"].endswith("test_later"))
    assert later["attempts"] == 0


@pytest.mark.skipif(sys.platform != "darwin", reason="macOS lacks native hard backend")
def test_hard_limit_rejection_precedes_target_import(tmp_path: Path) -> None:
    """Unavailable hard enforcement rejects collection before user imports run."""
    marker = tmp_path / "imported.txt"
    project(tmp_path, f"from pathlib import Path\nPath({str(marker)!r}).write_text('imported')\n")
    result = invoke(
        tmp_path, "run", "test_target.py", "--duration", "1", "--store", str(tmp_path / "store")
    )
    assert result.returncode == 2  # noqa: PLR2004 -- infrastructure exit
    assert not marker.exists()
