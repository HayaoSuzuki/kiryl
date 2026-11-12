# Copyright (c) 2026 kiryl contributors
"""Boundary evidence produces typed comparison and observation states."""

import hashlib
import json
import os
import subprocess
from pathlib import Path
from typing import TYPE_CHECKING, cast
from unittest.mock import create_autospec, patch

import pytest

import benchmark
from benchmark import BINARY, _run_mode, _versions
from evidence import probe_results
from model import BenchmarkMode, ComparisonTarget, ProbeStatus

if TYPE_CHECKING:
    from collections.abc import Iterator


@pytest.fixture
def comparison_root(tmp_path: Path) -> Path:
    """Provide independently authored process-boundary report evidence."""
    specimens = tmp_path / "sources" / "specimens"
    specimens.mkdir(parents=True)
    (specimens / "discovery.py").write_text("# comparison specimen\n")
    (specimens / "test_discovery.py").write_text("# collected specimen\n")
    return tmp_path


@pytest.fixture
def completed_comparison(request: pytest.FixtureRequest) -> Iterator[None]:
    """Write one report at the namespace selected by the real routing code."""
    outcomes = cast(
        "list[dict[str, str]]",
        getattr(request, "param", [{"kind": "failure", "verification": "confirmed"}]),
    )

    def complete(args: list[str], **_kwargs: object) -> subprocess.CompletedProcess[bytes]:
        namespace = Path(args[-1])
        (namespace / "report.json").write_text(
            json.dumps(
                {
                    "outcomes": outcomes,
                    "capabilities": {"backend": "portable"},
                    "first_discovery_ms": 1,
                    "attempts": 2,
                    "useful": 1,
                    "rejected": 0,
                    "branches": 1,
                    "corpus_entries": 2,
                    "elapsed_ms": 20,
                    "replay_ms": 1,
                    "shrink_ms": 1,
                    "adapter_ns": 3,
                    "dispatch_roundtrip_ns": 4,
                }
            )
        )
        return subprocess.CompletedProcess(args, 0)

    run = create_autospec(subprocess.run, spec_set=True, side_effect=complete)
    with patch.object(subprocess, "run", run):
        yield


@pytest.mark.parametrize(
    ("mode", "flags"),
    [
        (BenchmarkMode.GUIDED, (True, True, False)),
        (BenchmarkMode.UNGUIDED, (True, False, False)),
        (BenchmarkMode.HYPOTHESIS, (False, False, True)),
    ],
)
@pytest.mark.parametrize(
    ("target", "nodeid"),
    [
        (ComparisonTarget.BRANCHING, "test_discovery.py::test_branching"),
        (ComparisonTarget.STATEFUL, "test_discovery.py::TestStore::runTest"),
    ],
)
@pytest.mark.usefixtures("completed_comparison")
def test_comparison_mode_routes_real_configuration(
    comparison_root: Path,
    mode: BenchmarkMode,
    target: ComparisonTarget,
    nodeid: str,
    flags: tuple[bool, bool, bool],
) -> None:
    """Each typed algorithm selects the existing controller protocol flags."""
    search, feedback, baseline = flags
    result = _run_mode(comparison_root, target, mode, 0, 20)
    config = json.loads((comparison_root / f"{target}-{mode}" / "config.json").read_text())
    assert config["invocations"][0]["test_id"] == nodeid
    assert config["search"] is search
    assert config["feedback"] is feedback
    assert config["baseline"] is baseline
    assert result["target"] is target
    assert result["mode"] is mode
    assert result["confirmed_original"] is True
    assert json.loads(json.dumps(result))["mode"] == mode.value


@pytest.mark.parametrize(
    ("child", "expected"),
    [
        ("", ProbeStatus.PASSED),
        ("<skipped/>", ProbeStatus.UNVERIFIED),
        ("<failure/>", ProbeStatus.FAILED),
    ],
)
def test_junit_outcomes_decode_to_probe_states(
    tmp_path: Path, child: str, expected: ProbeStatus
) -> None:
    """A JUnit execution result becomes a typed gate observation."""
    suite = tmp_path / "probes.xml"
    suite.write_text(
        f'<testsuite><testcase name="test_real_fixture_returns_match_lean_generated_counts">'
        f"{child}</testcase></testsuite>"
    )
    assert probe_results(suite)["fixtures"] is expected


def test_comparison_uses_native_probe_name() -> None:
    """The launched comparison executable follows the native platform filename."""
    expected = {"posix": "fuzz-probe", "nt": "fuzz-probe.exe"}
    assert BINARY.name == expected[os.name]


@pytest.mark.parametrize(
    ("completed_comparison", "expected"),
    [
        ([{"kind": "future-result", "verification": "pending"}], False),
        (
            [
                {"kind": "future-result", "verification": "pending"},
                {"kind": "failure", "verification": "confirmed"},
            ],
            True,
        ),
    ],
    indirect=["completed_comparison"],
)
@pytest.mark.usefixtures("completed_comparison")
def test_future_outcomes_do_not_hide_known_failure_evidence(
    comparison_root: Path, *, expected: bool
) -> None:
    """Unknown classifications retain the old non-failure selection behavior."""
    result = _run_mode(comparison_root, ComparisonTarget.BRANCHING, BenchmarkMode.GUIDED, 0, 20)
    assert result["confirmed_original"] is expected


def test_comparison_snapshots_controller_state_definitions(tmp_path: Path) -> None:
    """Changing state codecs must change the retained comparison input fingerprint."""
    binary = tmp_path / "owned-probe"
    binary.write_bytes(b"owned probe fixture")
    versions = create_autospec(
        _versions,
        spec_set=True,
        return_value={"probe_binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest()},
    )
    run = create_autospec(_run_mode, spec_set=True, return_value={})
    validate = create_autospec(benchmark.validate, spec_set=True, return_value=[])
    with (
        patch.object(benchmark, "BINARY", binary),
        patch.object(benchmark, "_versions", versions),
        patch.object(benchmark, "_run_mode", run),
        patch.object(benchmark, "validate", validate),
    ):
        evidence = benchmark.run(tmp_path / "comparison", 0, 20)
    record = json.loads(evidence.read_text())
    source = (benchmark.ROOT / "src/types.rs").read_bytes()
    assert record["sources"]["sources/src/types.rs"] == hashlib.sha256(source).hexdigest()
    assert (evidence.parent / "sources/src/types.rs").read_bytes() == source
    assert record["source_changed"] is False
