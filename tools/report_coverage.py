# Copyright (c) 2026 kiryl contributors
"""Merge all six native reports without hiding unexecuted source locations."""

from __future__ import annotations

import argparse
import json
import re
from collections import defaultdict
from pathlib import Path

if bool(__package__):
    from .run_coverage import source_snapshot
else:
    from run_coverage import source_snapshot

ROOT = Path(__file__).resolve().parents[1]

# Rust v0 mangling embeds crate hashes that change between native target builds.
CRATE_HASH = re.compile(r"Cs[0-9A-Za-z]+_")
PLATFORMS = ("linux", "darwin", "win32")
PYTHONS = ("3.14", "3.15")
type Metrics = dict[str, dict[str, dict[str, int]]]


def source_name(path: str) -> str:
    """Keep Python and Rust tests distinct while removing checkout prefixes."""
    path = path.replace("\\", "/")
    for marker in (
        "python/kiryl_adapter/",
        "python/tests/",
        "kiryl_adapter/",
        "tools/",
        "src/",
        "tests/",
    ):
        if path.startswith(marker):
            return path
        boundary = "/" + marker
        if boundary in path:
            return marker + path.rsplit(boundary, 1)[1]
    msg = f"coverage source is outside the product scope: {path}"
    raise ValueError(msg)


def hit_count(value: str) -> int:
    """Reject negative hit counts instead of treating them as executed."""
    count = int(value)
    if count < 0:
        message = "coverage hit counts cannot be negative"
        raise ValueError(message)
    return count


def require_definitions(pending: dict[str, int]) -> None:
    """Never discard an undefined function count at record boundaries."""
    if pending:
        message = f"function counts lack definitions: {sorted(pending)}"
        raise ValueError(message)


def rust_counts(paths: list[Path]) -> Metrics:
    """Retain distinct functions and zero hits, including absent FNDA entries."""
    result: Metrics = {}
    for path in paths:
        filename = ""
        functions: dict[str, str] = {}
        pending: dict[str, int] = defaultdict(int)
        for line in path.read_text(encoding="utf-8").splitlines():
            tag, _, value = line.partition(":")
            if tag == "SF":
                require_definitions(pending)
                filename = source_name(value)
                result.setdefault(filename, {"lines": {}, "functions": {}})
                functions = {}
                pending = defaultdict(int)
            elif tag == "DA":
                number, count, *_ = value.split(",")
                entries = result[filename]["lines"]
                entries[number] = entries.get(number, 0) + hit_count(count)
            elif tag == "FN":
                number, name = value.split(",", 1)
                identity = f"{number}:{CRATE_HASH.sub('C_', name)}"
                functions[name] = identity
                entries = result[filename]["functions"]
                entries[identity] = entries.get(identity, 0) + pending.pop(name, 0)
            elif tag == "FNDA":
                count, name = value.split(",", 1)
                if name in functions:
                    entries = result[filename]["functions"]
                    identity = functions[name]
                    entries[identity] = entries.get(identity, 0) + hit_count(count)
                else:
                    pending[name] += hit_count(count)
            elif tag == "end_of_record":
                require_definitions(pending)
        require_definitions(pending)
    return result


def python_counts(paths: list[Path]) -> Metrics:
    """Union executable statements and branch arcs using exact hit presence."""
    result: Metrics = {}
    for path in paths:
        report = json.loads(path.read_text(encoding="utf-8"))
        if not report["meta"]["branch_coverage"]:
            msg = "Python reports must measure branches"
            raise ValueError(msg)
        for name, item in report["files"].items():
            source = source_name(name)
            if source.startswith(("tests/", "kiryl_adapter/")):
                source = f"python/{source}"
            counts = result.setdefault(source, {"lines": {}, "branches": {}})
            for metric, missing_key, executed_key in (
                ("lines", "missing_lines", "executed_lines"),
                ("branches", "missing_branches", "executed_branches"),
            ):
                for location in item[missing_key]:
                    counts[metric].setdefault(str(location), 0)
                for location in item[executed_key]:
                    key = str(location)
                    counts[metric][key] = counts[metric].get(key, 0) + 1
    return result


def missing(counts: Metrics) -> dict[str, dict[str, list[str]]]:
    """Show every exact zero-hit source location in stable source order."""
    return {
        name: {
            metric: sorted(location for location, hits in entries.items() if hits == 0)
            for metric, entries in metrics.items()
        }
        for name, metrics in sorted(counts.items())
        if any(hits == 0 for entries in metrics.values() for hits in entries.values())
    }


def report_directories(directory: Path) -> list[Path]:
    """Reject incomplete, mixed-revision or duplicate native matrix evidence."""
    directories = sorted(path.parent for path in directory.rglob("revision.txt"))
    revisions: set[str] = set()
    cells: set[tuple[str, str]] = set()
    snapshots: list[tuple[object, object]] = []
    for report in directories:
        revision = (report / "revision.txt").read_text(encoding="utf-8").strip()
        if not re.fullmatch(r"[a-f0-9]{40}", revision):
            msg = "each report must identify a full source revision"
            raise ValueError(msg)
        revisions.add(revision)
        metadata = json.loads((report / "native.json").read_text(encoding="utf-8"))
        cell = (metadata["platform"], metadata["python"])
        if cell in cells:
            msg = f"duplicate native report: {cell}"
            raise ValueError(msg)
        cells.add(cell)
        snapshots.append((metadata.get("source_hashes"), metadata.get("dirty")))
        if not (report / "rust.lcov").is_file() or not (report / "python.json").is_file():
            msg = f"incomplete native coverage report: {report}"
            raise ValueError(msg)
    if len(revisions) != 1:
        msg = "reports must identify exactly one tested source revision"
        raise ValueError(msg)
    expected = {(platform, version) for platform in PLATFORMS for version in PYTHONS}
    if cells != expected:
        msg = (
            f"all six native reports are required; missing={sorted(expected - cells)}, "
            f"unexpected={sorted(cells - expected)}"
        )
        raise ValueError(msg)
    discovered = set(directory.rglob("rust.lcov")) | set(directory.rglob("python.json"))
    paired = {report / name for report in directories for name in ("rust.lcov", "python.json")}
    if discovered != paired:
        msg = "coverage reports without matching revision metadata are forbidden"
        raise ValueError(msg)
    expected_sources = source_snapshot(ROOT)
    for sources, dirty in snapshots:
        if sources != expected_sources or not isinstance(dirty, bool):
            msg = (
                "native reports must match the complete current source snapshot "
                "and record dirty state"
            )
            raise ValueError(msg)
    return directories


def validate_counts(rust: Metrics, python: Metrics) -> None:
    """Fail closed when a report omits executable source or function evidence."""
    if (
        not any(item["lines"] for item in rust.values())
        or not any(item["functions"] for item in rust.values())
        or not any(item["lines"] for item in python.values())
    ):
        msg = "nonempty Rust line/function and Python statement reports are required"
        raise ValueError(msg)


def validate_inventory(rust: Metrics, python: Metrics) -> None:
    """Require the native union to contain every scoped executable source file."""
    sources = source_snapshot(ROOT)
    expected_rust = {name for name in sources if name.endswith(".rs")}
    library = ROOT / "src/lib.rs"
    if library.is_file() and all(
        not line.strip()
        or line.lstrip().startswith("//")
        or re.fullmatch(r"(?:pub )?mod \w+;", line.strip())
        for line in library.read_text(encoding="utf-8").splitlines()
    ):
        expected_rust.discard("src/lib.rs")
    expected_python = {
        name
        for name in sources
        if name.startswith(("python/kiryl_adapter/", "python/tests/"))
        or name in ("tools/run_coverage.py", "tools/report_coverage.py")
    }
    missing_rust = expected_rust - rust.keys()
    missing_python = expected_python - python.keys()
    if missing_rust or missing_python:
        msg = (
            f"source files omitted from coverage: Rust={sorted(missing_rust)}, "
            f"Python={sorted(missing_python)}"
        )
        raise ValueError(msg)


def main() -> None:
    """Gate the complete native union at exactly 100 percent."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    arguments = parser.parse_args()
    try:
        directories = report_directories(arguments.directory)
        rust = rust_counts([path / "rust.lcov" for path in directories])
        python = python_counts([path / "python.json" for path in directories])
        validate_counts(rust, python)
        validate_inventory(rust, python)
    except (ValueError, KeyError, OSError) as error:
        parser.error(str(error))
    failures = {"rust": missing(rust), "python": missing(python)}
    print(json.dumps(failures, indent=2))  # noqa: T201 - command output is the coverage evidence
    if failures["rust"] or failures["python"]:
        message = "Coverage must be exactly 100%; unexecuted locations are listed above"
        raise SystemExit(message)
    print("Rust lines/functions and Python statements/branches: exactly 100%, including tests")  # noqa: T201 - CLI result


if __name__ == "__main__":
    main()
