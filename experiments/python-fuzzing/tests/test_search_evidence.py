# Copyright (c) 2026 kiryl contributors
"""Evidence validation uses explicit synthetic fixtures, never platform claims."""

import hashlib
import json
import sys
import xml.etree.ElementTree as ET
from typing import TYPE_CHECKING, cast

import pytest

from evidence import validate

if TYPE_CHECKING:
    from pathlib import Path

MODES = ("guided", "unguided", "hypothesis")
TARGETS = ("branching", "stateful")


def _record(root: Path) -> tuple[Path, dict[str, object]]:
    source = root / "fixture.py"
    source.write_text("# independently authored validator fixture\n")
    environment = {
        "platform": sys.platform,
        "python": list(sys.version_info[:2]),
        "implementation": "cpython",
    }
    runs = []
    for target in TARGETS:
        for mode in MODES:
            namespace = root / f"{target}-{mode}"
            namespace.mkdir()
            (namespace / "database").mkdir()
            (namespace / "corpus").mkdir()
            report = namespace / "report.json"
            report.write_text(
                json.dumps(
                    {
                        "version": 1,
                        "environment": environment,
                        "attempts": 1,
                        "mode": mode,
                        "seed": 0,
                    }
                )
            )
            runs.append(
                {
                    "mode": mode,
                    "target": target,
                    "seed": 0,
                    "budget_seconds": 20,
                    "storage": namespace.name,
                    "database": f"{namespace.name}/database",
                    "corpus": f"{namespace.name}/corpus",
                    "initial_database_entries": [],
                    "report": f"{namespace.name}/report.json",
                    "report_sha256": hashlib.sha256(report.read_bytes()).hexdigest(),
                }
            )
    record: dict[str, object] = {
        "version": 1,
        "environment": environment,
        "native": True,
        "completed_cells": [],
        "seed": 0,
        "probes": dict.fromkeys(
            ("fixtures", "stateful", "replay", "crash", "logs", "monitoring", "hard_memory"),
            "unverified",
        ),
        "sources": {"fixture.py": hashlib.sha256(source.read_bytes()).hexdigest()},
        "versions": {"hypothesis": "fixture", "pytest": "fixture", "rust": "fixture"},
        "runs": runs,
    }
    path = root / "evidence.json"
    path.write_text(json.dumps(record))
    return path, record


def test_complete_observation_shape_validates(tmp_path: Path) -> None:
    """A complete record shape may honestly retain unverified platform gates."""
    path, _ = _record(tmp_path)
    assert validate(path) == []


def test_unrun_cell_is_rejected(tmp_path: Path) -> None:
    """Execution records for one interpreter cannot complete another OS/minor."""
    path, record = _record(tmp_path)
    record["completed_cells"] = ["unrun-platform-3.15"]
    path.write_text(json.dumps(record))
    assert validate(path)


def test_missing_source_provenance_is_rejected(tmp_path: Path) -> None:
    """A bundle cannot establish identity without its source provenance."""
    path, record = _record(tmp_path)
    del record["sources"]
    path.write_text(json.dumps(record))
    assert validate(path)


def test_storage_overlap_is_rejected(tmp_path: Path) -> None:
    """Comparisons require independent database namespaces."""
    path, record = _record(tmp_path)
    runs = cast("list[dict[str, object]]", record["runs"])
    runs[2]["database"] = runs[0]["database"]
    path.write_text(json.dumps(record))
    assert validate(path)


def test_nonempty_initial_database_is_rejected(tmp_path: Path) -> None:
    """Previously stored choices invalidate a fresh discovery comparison."""
    path, record = _record(tmp_path)
    runs = cast("list[dict[str, object]]", record["runs"])
    runs[2]["initial_database_entries"] = ["existing-choice"]
    path.write_text(json.dumps(record))
    assert validate(path)


def test_report_tampering_is_rejected(tmp_path: Path) -> None:
    """A source hash cannot certify report bytes changed after recording."""
    path, _ = _record(tmp_path)
    (tmp_path / "branching-guided/report.json").write_text("{}")
    assert validate(path)


def test_missing_probe_category_is_rejected(tmp_path: Path) -> None:
    """A complete comparison retains every required acceptance category."""
    path, record = _record(tmp_path)
    probes = cast("dict[str, str]", record["probes"])
    del probes["crash"]
    path.write_text(json.dumps(record))
    assert validate(path)


@pytest.mark.parametrize(("field", "value"), [("mode", "unknown"), ("target", "unknown")])
def test_unknown_comparison_labels_are_rejected(tmp_path: Path, field: str, value: str) -> None:
    """Unknown external routing labels cannot create a complete comparison."""
    path, record = _record(tmp_path)
    runs = cast("list[dict[str, object]]", record["runs"])
    runs[0][field] = value
    path.write_text(json.dumps(record))
    assert "invalid comparison mode/target" in validate(path)


@pytest.fixture(params=["skipped", "failure", "missing"])
def contradictory_probe_record(tmp_path: Path, request: pytest.FixtureRequest) -> Path:
    """Build one independently selected contradictory JUnit report fixture."""
    contradiction = request.param
    from evidence import PROBE_TESTS  # noqa: PLC0415 -- shared extraction contract

    path, record = _record(tmp_path)
    root = ET.Element("testsuites")
    suite = ET.SubElement(root, "testsuite")
    properties = ET.SubElement(suite, "properties")
    environment = cast("dict[str, object]", record["environment"])
    for key, value in {
        "platform": environment["platform"],
        "python": ".".join(map(str, cast("list[int]", environment["python"]))),
        "implementation": environment["implementation"],
    }.items():
        ET.SubElement(properties, "property", name=f"kiryl.{key}", value=str(value))
    for name in sorted({name for names in PROBE_TESTS.values() for name in names}):
        case = ET.SubElement(suite, "testcase", name=name)
        if name == "test_combined_memory_requires_backend_evidence":
            if contradiction == "missing":
                suite.remove(case)
            else:
                ET.SubElement(case, contradiction)
    xml = tmp_path / "probes.xml"
    ET.ElementTree(root).write(xml)
    record["probe_source"] = {
        "path": xml.name,
        "sha256": hashlib.sha256(xml.read_bytes()).hexdigest(),
    }
    record["probes"] = dict.fromkeys(PROBE_TESTS, "passed")
    record["completed_cells"] = [
        f"{sys.platform}-{sys.version_info.major}.{sys.version_info.minor}"
    ]
    path.write_text(json.dumps(record))
    return path


def test_probe_declarations_must_match_junit(contradictory_probe_record: Path) -> None:
    """A valid hash cannot promote failed, skipped or absent testcases to passed."""
    assert "probe declarations contradict JUnit outcomes" in validate(contradictory_probe_record)
