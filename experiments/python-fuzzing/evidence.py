# Copyright (c) 2026 kiryl contributors
"""Check local evidence integrity without inferring unobserved platform success."""

import hashlib
import json
import xml.etree.ElementTree as ET
from typing import TYPE_CHECKING, cast

if TYPE_CHECKING:
    from pathlib import Path

BUDGET_SECONDS = 20

PROBES = frozenset({"fixtures", "stateful", "replay", "crash", "logs", "monitoring", "hard_memory"})
MODES = frozenset({"guided", "unguided", "hypothesis"})
TARGETS = frozenset({"branching", "stateful"})


PROBE_TESTS = {
    "fixtures": {"test_real_fixture_returns_match_lean_generated_counts"},
    "stateful": {"test_stateful_semantic_shrinking"},
    "replay": {
        "test_ordinary_pytest_reuses_database",
        "test_measured_ordinary_path_reuses_original_database",
    },
    "crash": {"test_crash_retains_exact_original_and_continues"},
    "logs": {
        "test_output_is_bounded_and_control_survives",
        "test_flood_remains_bounded_when_killed",
    },
    "monitoring": {
        "test_other_tools_keep_disabled_events",
        "test_feature_ids_match_an_independent_interpreter",
    },
    "hard_memory": {
        "test_combined_memory_requires_backend_evidence",
        "test_setup_memory_limit_is_infrastructure",
        "test_two_trees_have_independent_allowances",
    },
}


def probe_results(path: Path | None) -> dict[str, str]:
    """Extract only observed pytest outcomes; missing/skipped tests stay unverified."""
    statuses = dict.fromkeys(PROBE_TESTS, "unverified")
    if path is None:
        return statuses
    tree = ET.parse(path)  # noqa: S314 -- locally generated, owned pytest JUnit file
    observed: dict[str, list[str]] = {}
    for case in tree.iter("testcase"):
        name = case.get("name", "").split("[", 1)[0]
        status = (
            "failed"
            if case.find("failure") is not None or case.find("error") is not None
            else "unverified"
            if case.find("skipped") is not None
            else "passed"
        )
        observed.setdefault(name, []).append(status)
    for category, names in PROBE_TESTS.items():
        values = [state for name in names for state in observed.get(name, ["unverified"])]
        statuses[category] = (
            "failed" if "failed" in values else "unverified" if "unverified" in values else "passed"
        )
    return statuses


def _object(value: object) -> dict[str, object]:
    if not isinstance(value, dict):
        message = "expected an object"
        raise TypeError(message)
    return cast("dict[str, object]", value)


def _local(root: Path, value: object) -> Path:
    if not isinstance(value, str) or not value:
        message = "missing local evidence path"
        raise ValueError(message)
    path = (root / value).resolve()
    if not path.is_relative_to(root):
        message = "evidence path escapes bundle"
        raise ValueError(message)
    return path


def _digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _probe_provenance(
    root: Path, value: dict[str, object], environment: dict[str, object]
) -> list[str]:
    errors = []
    python = environment.get("python")
    provenance = _object(value.get("probe_source"))
    suite = _local(root, provenance.get("path"))
    if _digest(suite) != provenance.get("sha256"):
        errors.append("changed probe evidence")
    xml = ET.parse(suite)  # noqa: S314 -- owned pytest evidence, hash checked above
    properties = {entry.get("name"): entry.get("value") for entry in xml.iter("property")}
    expected = {
        "kiryl.platform": environment.get("platform"),
        "kiryl.python": ".".join(map(str, cast("list[int]", python))),
        "kiryl.implementation": environment.get("implementation"),
    }
    if any(properties.get(key) != expected_value for key, expected_value in expected.items()):
        errors.append("probe interpreter/platform mismatch")
    if value.get("probes") != probe_results(suite):
        errors.append("probe declarations contradict JUnit outcomes")
    return errors


def _provenance(root: Path, value: dict[str, object]) -> list[str]:
    errors = []
    if value.get("source_changed", False):
        errors.append("source/binary changed during comparisons")
    if type(value.get("version")) is not int or value["version"] != 1:
        errors.append("incompatible evidence schema")
    environment = _object(value.get("environment"))
    python = environment.get("python")
    if (
        environment.get("platform") not in {"darwin", "linux", "win32"}
        or python not in ([3, 14], [3, 15])
        or environment.get("implementation") != "cpython"
        or value.get("native") is not True
    ):
        errors.append("missing native provenance")
    probes = _object(value.get("probes"))
    if set(probes) != PROBES or any(
        v not in {"passed", "failed", "unverified"} for v in probes.values()
    ):
        errors.append("missing/invalid required probe categories")
    if "probe_source" in value or any(status != "unverified" for status in probes.values()):
        errors.extend(_probe_provenance(root, value, environment))
    completed = value.get("completed_cells")
    own = f"{environment.get('platform')}-{'.'.join(map(str, cast('list[int]', python)))}"
    if not isinstance(completed, list) or any(cell != own for cell in completed):
        errors.append("unobserved platform completion")
    elif completed and any(status != "passed" for status in probes.values()):
        errors.append("unverified gate cannot complete a platform")
    sources = _object(value.get("sources"))
    if not sources or any(
        _digest(_local(root, name)) != digest for name, digest in sources.items()
    ):
        errors.append("missing/changed source provenance")
    versions = _object(value.get("versions"))
    if any(not versions.get(key) for key in ("hypothesis", "pytest", "rust")):
        errors.append("missing version provenance")
    return errors


def _comparison_paths(
    root: Path, run: dict[str, object], environment: dict[str, object]
) -> tuple[Path, list[str]]:
    errors = []
    namespace = _local(root, run.get("storage"))
    database, corpus = (_local(root, run.get(key)) for key in ("database", "corpus"))
    if database.parent != namespace or corpus.parent != namespace or database == corpus:
        errors.append("database/corpus outside independent namespace")
    if run.get("initial_database_entries") != []:
        errors.append("comparison database was not fresh")
    report_path = _local(root, run.get("report"))
    if _digest(report_path) != run.get("report_sha256"):
        errors.append("changed report bytes")
    if not report_path.is_relative_to(namespace):
        errors.append("report outside comparison namespace")
    report = _object(json.loads(report_path.read_text()))
    if (
        type(report.get("version")) is not int
        or report.get("version") != 1
        or report.get("environment") != environment
        or report.get("mode") != run.get("mode")
        or report.get("seed") != run.get("seed")
    ):
        errors.append("report provenance mismatch")
    return namespace, errors


def _comparisons(root: Path, value: dict[str, object]) -> list[str]:
    errors = []
    environment = _object(value.get("environment"))
    runs = value.get("runs")
    if not isinstance(runs, list):
        return [*errors, "missing runs"]
    namespaces: list[Path] = []
    keys: set[tuple[str, str]] = set()
    for raw in runs:
        run = _object(raw)
        mode, target = run.get("mode"), run.get("target")
        if mode not in MODES or target not in TARGETS:
            errors.append("invalid comparison mode/target")
        else:
            key = (cast("str", target), cast("str", mode))
            if key in keys:
                errors.append("duplicate comparison")
            keys.add(key)
        if run.get("seed") != value.get("seed") or run.get("budget_seconds") != BUDGET_SECONDS:
            errors.append("comparison seed/budget mismatch")
        namespace, path_errors = _comparison_paths(root, run, environment)
        errors.extend(path_errors)
        if any(
            namespace.is_relative_to(other) or other.is_relative_to(namespace)
            for other in namespaces
        ):
            errors.append("overlapping comparison namespace")
        namespaces.append(namespace)
    if keys != {(target, mode) for target in TARGETS for mode in MODES}:
        errors.append("incomplete comparisons")
    return errors


def _validate(path: Path) -> list[str]:
    root = path.parent.resolve()
    value = _object(json.loads(path.read_text()))
    return _provenance(root, value) + _comparisons(root, value)


def validate(path: Path) -> list[str]:
    """Return schema/integrity errors; never turn unreadable evidence into success."""
    try:
        return _validate(path)
    except (OSError, ValueError, TypeError, KeyError, ET.ParseError) as error:
        return [f"invalid evidence: {error}"]
