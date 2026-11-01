# Copyright (c) 2026 kiryl contributors
"""Run bounded, isolated synthetic comparisons; never supply known failure seeds."""

import argparse
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
from importlib.metadata import version
from pathlib import Path
from typing import cast

from evidence import BUDGET_SECONDS, probe_results, validate

ROOT = Path(__file__).resolve().parent
INFRASTRUCTURE_EXIT = 2

BINARY = ROOT / "target/debug" / ("fuzz-probe.exe" if os.name == "nt" else "fuzz-probe")
SOURCES = (
    "adapter.py",
    "hypothesis_bridge.py",
    "monitor.py",
    "worker.py",
    "wire.py",
    "benchmark.py",
    "evidence.py",
    "pyproject.toml",
    "uv.lock",
    "Cargo.toml",
    "Cargo.lock",
    "src/lib.rs",
    "src/controller.rs",
    "src/search.rs",
    "src/artifacts.rs",
    "src/wire.rs",
    "src/main.rs",
    "src/resource/mod.rs",
    "src/resource/linux.rs",
    "src/resource/windows.rs",
    "src/resource/portable.rs",
    "specimens/discovery.py",
    "specimens/test_discovery.py",
)


def _digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _versions() -> dict[str, str]:
    executable = shutil.which("rustc")
    if executable is None:
        message = "rustc is unavailable"
        raise OSError(message)
    compiler = subprocess.run(  # noqa: S603 -- resolved metadata tool, no target data
        [executable, "--version"], capture_output=True, text=True, timeout=10, check=True
    )
    return {
        "hypothesis": version("hypothesis"),
        "pytest": version("pytest"),
        "python": sys.version,
        "rust": compiler.stdout.strip(),
        "platform": platform.platform(),
        "machine": platform.machine(),
        "gil_enabled": str(sys._is_gil_enabled()),  # noqa: SLF001 -- documented CPython runtime GIL query
        "probe_binary_sha256": _digest(BINARY),
    }


def _run_mode(
    root: Path, target: str, mode: str, seed: int, budget_seconds: float
) -> dict[str, object]:
    namespace = root / f"{target}-{mode}"
    workspace = namespace / "workspace"
    workspace.mkdir(parents=True)
    for name in ("discovery.py", "test_discovery.py"):
        shutil.copyfile(root / "sources/specimens" / name, workspace / name)
    (workspace / "pytest.ini").write_text("[pytest]\n", encoding="utf-8")
    database = namespace / "database"
    database.mkdir()
    (namespace / "corpus").mkdir()
    initial = list(database.iterdir())
    suffix = "test_branching" if target == "branching" else "TestStore::runTest"
    config = {
        "python": sys.executable,
        "pytest_args": [
            f"{workspace / 'test_discovery.py'}::{suffix}",
            "-q",
            "-s",
            f"--hypothesis-seed={seed}",
        ],
        "invocations": [{"test_id": f"test_discovery.py::{suffix}", "inputs": []}],
        "limits": {
            "campaign_ms": int(budget_seconds * 1000),
            "allow_best_effort": True,
            "retained_output_bytes": 4096,
        },
        "seed": seed,
        "search": mode != "hypothesis",
        "feedback": mode == "guided",
        "baseline": mode == "hypothesis",
    }
    config_path = namespace / "config.json"
    config_path.write_text(json.dumps(config), encoding="utf-8")
    completed = subprocess.run(  # noqa: S603 -- owned bounded probe, independently authored specimens
        [str(BINARY), "run", "--config", str(config_path), "--out", str(namespace)],
        env={**os.environ, "KIRYL_BENCHMARK_DATABASE": str(database)},
        capture_output=True,
        timeout=budget_seconds + 10,
        check=False,
    )
    report_path = namespace / "report.json"
    report = cast("dict[str, object]", json.loads(report_path.read_text(encoding="utf-8")))
    outcomes = cast("list[dict[str, object]]", report["outcomes"])
    finding = next((row for row in outcomes if row["kind"] == "failure"), None)
    return {
        "mode": mode,
        "target": target,
        "seed": seed,
        "budget_seconds": budget_seconds,
        "storage": namespace.name,
        "database": f"{namespace.name}/database",
        "corpus": f"{namespace.name}/corpus",
        "initial_database_entries": [path.name for path in initial],
        "report": f"{namespace.name}/report.json",
        "report_sha256": _digest(report_path),
        "capabilities": report["capabilities"],
        "exit_code": completed.returncode,
        "discovered": report["first_discovery_ms"] is not None,
        "confirmed_original": finding is not None and finding["verification"] == "confirmed",
        "attempts": report["attempts"],
        "useful": report["useful"],
        "rejected": report["rejected"],
        "branches": report["branches"],
        "corpus_entries": report["corpus_entries"],
        "first_discovery_ms": report["first_discovery_ms"],
        "elapsed_ms": report["elapsed_ms"],
        "replay_ms": report["replay_ms"],
        "shrink_ms": report["shrink_ms"],
        "adapter_ns": report["adapter_ns"],
        "dispatch_roundtrip_ns": report["dispatch_roundtrip_ns"],
        "counts_scope": "ordinary oracle calls including replay/shrink"
        if mode == "hypothesis"
        else "search cases excluding verification/shrink",
        "adapter_scope": "inner oracle and monitor, excludes generation"
        if mode == "hypothesis"
        else "bridge including draws and monitor",
    }


def run(output: Path, seed: int, budget_seconds: float) -> Path:
    """Run one seed's six comparisons with fresh, disjoint database/corpus parents."""
    if type(seed) is not int or seed not in range(5) or budget_seconds != BUDGET_SECONDS:
        message = "formal comparison requires seeds0..4 and exactly20 seconds per test/mode"
        raise ValueError(message)
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    sources = {}
    for relative in SOURCES:
        destination = output / "sources" / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / relative, destination)
        sources[f"sources/{relative}"] = _digest(destination)
    versions = _versions()
    runs = []
    for target in ("branching", "stateful"):
        # Rotate comparison order deterministically rather than always favoring guided first.
        modes = ("guided", "unguided", "hypothesis")
        rotation = seed % len(modes)
        runs.extend(
            _run_mode(output, target, mode, seed, budget_seconds)
            for mode in modes[rotation:] + modes[:rotation]
        )
    environment = {
        "platform": sys.platform,
        "python": list(sys.version_info[:2]),
        "implementation": sys.implementation.name,
    }
    record = {
        "version": 1,
        "environment": environment,
        "native": True,
        "completed_cells": [],
        "seed": seed,
        "versions": versions,
        "sources": sources,
        "runs": runs,
        "probes": probe_results(None),
        "source_changed": any(
            _digest(ROOT / name) != sources[f"sources/{name}"] for name in SOURCES
        )
        or _digest(BINARY) != versions["probe_binary_sha256"],
        "timing_note": (
            "roundtrip minus adapter includes serialization, pipe exchange and scheduling; "
            "it is not pure IPC latency"
        ),
        "corpus_note": (
            "mutation corpus is bounded in memory; directories reserve disjoint comparison "
            "storage and are not a resume format"
        ),
    }
    path = output / "evidence.json"
    path.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    errors = validate(path)
    if errors:
        raise ValueError(errors)
    return path


def summarize(output: Path) -> Path:
    """Extract compact observations after validating all five raw seed bundles."""
    rows: list[dict[str, object]] = []
    evidence_hashes = {}
    records = []
    for seed in range(5):
        path = output / f"seed-{seed}/evidence.json"
        errors = validate(path)
        if errors:
            raise ValueError(errors)
        record = cast("dict[str, object]", json.loads(path.read_text(encoding="utf-8")))
        records.append(record)
        evidence_hashes[str(seed)] = _digest(path)
        rows.extend(
            {
                key: value
                for key, value in run_record.items()
                if key
                not in {"storage", "database", "corpus", "report", "initial_database_entries"}
            }
            for run_record in cast("list[dict[str, object]]", record["runs"])
        )
    first = records[0]
    if any(
        record["environment"] != first["environment"]
        or record["sources"] != first["sources"]
        or record["versions"] != first["versions"]
        for record in records
    ):
        message = "native seed observations do not share provenance"
        raise ValueError(message)
    probes = cast("dict[str, str]", first["probes"])
    guided = [row for row in rows if row["mode"] == "guided"]
    passed = all(status == "passed" for status in probes.values()) and all(
        row["confirmed_original"] and cast("int", row["corpus_entries"]) > 1 for row in guided
    )
    failed = any(status == "failed" for status in probes.values()) or any(
        row["exit_code"] == INFRASTRUCTURE_EXIT for row in rows
    )
    result = {
        "version": 1,
        "kind": "comparison_summary",
        "environment": first["environment"],
        "versions": first["versions"],
        "sources": first["sources"],
        "probes": probes,
        "status": "failed" if failed else "passed" if passed else "partial",
        "observed_seeds": list(range(5)),
        "budget_seconds": BUDGET_SECONDS,
        "raw_evidence_sha256": evidence_hashes,
        "samples": rows,
        "counts_note": (
            "Ordinary counts are inner oracle calls including replay/shrink, excluding "
            "generator rejections before oracle entry; search counts exclude replay/shrink. "
            "Use wall budgets for comparison; no statistical superiority claim."
        ),
    }
    path = output / "summary.json"
    path.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    return path


def main() -> int:
    """Run all fixed seeds and attach actual suite evidence when supplied."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--junit", type=Path)
    args = parser.parse_args()
    statuses = probe_results(args.junit)
    for seed in range(5):
        path = run(args.out / f"seed-{seed}", seed, BUDGET_SECONDS)
        record = cast("dict[str, object]", json.loads(path.read_text(encoding="utf-8")))
        record["probes"] = statuses
        if args.junit:
            shutil.copyfile(args.junit, path.parent / "probes.xml")
            record["probe_source"] = {
                "path": "probes.xml",
                "sha256": _digest(path.parent / "probes.xml"),
            }
        path.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
        errors = validate(path)
        print(json.dumps({"seed": seed, "evidence": str(path), "errors": errors}), flush=True)  # noqa: T201 -- structured CLI result
        if errors:
            return 2
    summarize(args.out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
