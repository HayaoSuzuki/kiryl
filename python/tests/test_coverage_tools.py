# Copyright (c) 2026 kiryl contributors
"""Exercise coverage aggregation and orchestration without recursive subprocesses."""

from __future__ import annotations

import itertools
import json
import runpy
import subprocess
import sys
from pathlib import Path
from typing import TYPE_CHECKING
from unittest.mock import MagicMock

import pytest
from tools import report_coverage, run_coverage

if TYPE_CHECKING:
    from collections.abc import Iterator

SHA = "a" * 40
NATIVE_CELLS = 6
PARSER_ERROR = 2
COMMAND_FAILURE = 7
LCOV = "SF:/checkout/src/main.rs\nFN:1,main\nFNDA:1,main\nDA:1,1\nend_of_record\n"
PYTHON = {
    "meta": {"branch_coverage": True},
    "files": {
        "tests/test_case.py": {
            "missing_lines": [],
            "executed_lines": [1],
            "missing_branches": [],
            "executed_branches": [[1, 2]],
        }
    },
    "totals": {},
}


@pytest.fixture
def matrix(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    """Build one complete six-cell report matrix."""
    checkout = tmp_path / "checkout"
    (checkout / "src").mkdir(parents=True)
    (checkout / "python/tests").mkdir(parents=True)
    (checkout / "tools").mkdir()
    (checkout / "src/main.rs").write_text("fn main() {}\n", encoding="utf-8")
    (checkout / "src/lib.rs").write_text(
        "//! declarations\n\npub mod main;\nmod support;\n", encoding="utf-8"
    )
    (checkout / "python/tests/test_case.py").write_text("pass\n", encoding="utf-8")
    (checkout / "tools/run_coverage.py").write_text("pass\n", encoding="utf-8")
    (checkout / "tools/report_coverage.py").write_text("pass\n", encoding="utf-8")
    monkeypatch.setattr(report_coverage, "ROOT", checkout)
    python_report = json.loads(json.dumps(PYTHON))
    python_report["files"]["/repo/tools/run_coverage.py"] = PYTHON["files"]["tests/test_case.py"]
    python_report["files"]["/repo/tools/report_coverage.py"] = PYTHON["files"]["tests/test_case.py"]
    for platform, version in itertools.product(report_coverage.PLATFORMS, report_coverage.PYTHONS):
        directory = tmp_path / f"{platform}-{version}"
        directory.mkdir()
        (directory / "revision.txt").write_text(SHA, encoding="utf-8")
        (directory / "native.json").write_text(
            json.dumps(
                {
                    "platform": platform,
                    "python": version,
                    "dirty": False,
                    "source_hashes": run_coverage.source_snapshot(checkout),
                }
            ),
            encoding="utf-8",
        )
        (directory / "rust.lcov").write_text(LCOV, encoding="utf-8")
        (directory / "python.json").write_text(json.dumps(python_report), encoding="utf-8")
    return tmp_path


@pytest.mark.parametrize(
    ("path", "expected"),
    [
        ("/runner/repo/src/main.rs", "src/main.rs"),
        (r"C:\runner\repo\tests\report.rs", "tests/report.rs"),
        ("/repo/python/tests/test_wire.py", "python/tests/test_wire.py"),
        ("python/kiryl_adapter/wire.py", "python/kiryl_adapter/wire.py"),
        ("kiryl_adapter/wire.py", "kiryl_adapter/wire.py"),
        ("../tools/run_coverage.py", "tools/run_coverage.py"),
        ("src/main.rs", "src/main.rs"),
    ],
)
def test_source_paths_normalize(path: str, expected: str) -> None:
    """Native roots preserve product-relative source identity."""
    assert report_coverage.source_name(path) == expected


def test_unknown_source_path_is_rejected() -> None:
    """Unrecognized paths cannot silently escape product scope."""
    with pytest.raises(ValueError, match="outside the product scope"):
        report_coverage.source_name("/dependency/vendor.py")


@pytest.mark.parametrize(
    "records",
    [
        "FN:1,hit\nFN:1,miss\nFNDA:1,hit\nFNDA:0,miss\n",
        "FNDA:1,hit\nFN:1,hit\nFN:1,miss\n",
    ],
)
def test_distinct_functions_on_one_line_keep_zero_hits(tmp_path: Path, records: str) -> None:
    """A neighboring hit must not hide a zero-hit function."""
    report = tmp_path / "rust.lcov"
    report.write_text("SF:src/main.rs\n" + records + "DA:1,1\nend_of_record\n", encoding="utf-8")
    assert report_coverage.missing(report_coverage.rust_counts([report])) == {
        "src/main.rs": {"lines": [], "functions": ["1:miss"]},
    }


def test_function_hashes_merge_across_native_targets(tmp_path: Path) -> None:
    """Crate disambiguators do not make native coverage entries unrelated."""
    first = tmp_path / "first.lcov"
    second = tmp_path / "second.lcov"
    first.write_text(
        LCOV.replace("main\n", "_RNvCsabc_4main\n").replace("FNDA:1", "FNDA:0"), encoding="utf-8"
    )
    second.write_text(LCOV.replace("main\n", "_RNvCsxyz_4main\n"), encoding="utf-8")
    assert report_coverage.missing(report_coverage.rust_counts([first, second])) == {}


def test_unknown_function_count_is_rejected(tmp_path: Path) -> None:
    """LCOV cannot discard hits lacking a function definition."""
    report = tmp_path / "rust.lcov"
    report.write_text("SF:src/main.rs\nFNDA:1,unknown\nend_of_record\n", encoding="utf-8")
    with pytest.raises(ValueError, match="lack definitions"):
        report_coverage.rust_counts([report])


def test_python_missing_locations_remain_visible(tmp_path: Path) -> None:
    """Missing statement and branch locations both remain in the gate."""
    report = tmp_path / "python.json"
    report.write_text(
        json.dumps(
            {
                "meta": {"branch_coverage": True},
                "files": {
                    "tests/test_case.py": {
                        "missing_lines": [2],
                        "executed_lines": [1],
                        "missing_branches": [[1, 3]],
                        "executed_branches": [[1, 2]],
                    },
                },
            }
        ),
        encoding="utf-8",
    )
    assert report_coverage.missing(report_coverage.python_counts([report])) == {
        "python/tests/test_case.py": {"lines": ["2"], "branches": ["[1, 3]"]},
    }


def test_python_absolute_tool_source_is_preserved(tmp_path: Path) -> None:
    """Tools stay distinct from product adapter and test paths."""
    report = tmp_path / "python.json"
    report.write_text(
        json.dumps(PYTHON).replace("tests/test_case.py", "/repo/tools/run_coverage.py"),
        encoding="utf-8",
    )
    assert "tools/run_coverage.py" in report_coverage.python_counts([report])


def test_python_branch_measurement_is_required(tmp_path: Path) -> None:
    """Statement-only evidence cannot satisfy a branch gate."""
    report = tmp_path / "python.json"
    report.write_text(json.dumps({"meta": {"branch_coverage": False}}), encoding="utf-8")
    with pytest.raises(ValueError, match="measure branches"):
        report_coverage.python_counts([report])


def test_complete_native_matrix_is_admitted(matrix: Path) -> None:
    """Both Python versions on every native platform are required."""
    assert len(report_coverage.report_directories(matrix)) == NATIVE_CELLS


@pytest.mark.parametrize(
    ("name", "contents", "expected"),
    [
        ("revision.txt", "short", "full source revision"),
        ("revision.txt", "b" * 40, "exactly one tested source revision"),
        ("native.json", '{"platform":"other","python":"3.14"}', "all six native"),
        ("native.json", '{"platform":"linux","python":"3.15"}', "duplicate native"),
    ],
)
def test_invalid_native_metadata_is_rejected(
    matrix: Path, name: str, contents: str, expected: str
) -> None:
    """Inconsistent evidence fails before aggregating locations."""
    (matrix / "linux-3.14" / name).write_text(contents, encoding="utf-8")
    with pytest.raises(ValueError, match=expected):
        report_coverage.report_directories(matrix)


@pytest.mark.parametrize("name", ["rust.lcov", "python.json"])
def test_missing_native_report_is_rejected(matrix: Path, name: str) -> None:
    """A cell requires both language reports."""
    (matrix / "linux-3.14" / name).unlink()
    with pytest.raises(ValueError, match="incomplete native"):
        report_coverage.report_directories(matrix)


def test_report_without_revision_is_rejected(matrix: Path) -> None:
    """Orphan coverage cannot borrow another cell's revision."""
    (matrix / "rust.lcov").write_text(LCOV, encoding="utf-8")
    with pytest.raises(ValueError, match="without matching revision"):
        report_coverage.report_directories(matrix)


def test_empty_matrix_is_rejected(tmp_path: Path) -> None:
    """No metadata is not a successful matrix."""
    with pytest.raises(ValueError, match="exactly one tested source revision"):
        report_coverage.report_directories(tmp_path)


@pytest.mark.parametrize(
    ("rust", "python"),
    [
        ({}, {}),
        ({"src/main.rs": {"lines": {"1": 1}, "functions": {}}}, {}),
        ({"src/main.rs": {"lines": {"1": 1}, "functions": {"main": 1}}}, {}),
    ],
)
def test_empty_metrics_are_rejected(
    rust: report_coverage.Metrics, python: report_coverage.Metrics
) -> None:
    """Empty measurements do not imply 100 percent coverage."""
    with pytest.raises(ValueError, match="nonempty"):
        report_coverage.validate_counts(rust, python)


def test_aggregate_cli_succeeds(
    matrix: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    """The gate accepts complete evidence with no missed locations."""
    monkeypatch.setattr(sys, "argv", ["report_coverage", str(matrix)])
    report_coverage.main()
    assert "exactly 100%" in capsys.readouterr().out


def test_aggregate_cli_reports_missed_location(
    matrix: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    """A zero-hit source location fails the aggregate gate."""
    (matrix / "linux-3.14/rust.lcov").write_text(
        LCOV + "SF:src/only.rs\nDA:3,0\nend_of_record\n", encoding="utf-8"
    )
    monkeypatch.setattr(sys, "argv", ["report_coverage", str(matrix)])
    with pytest.raises(SystemExit, match="Coverage must be exactly 100%"):
        report_coverage.main()
    assert "src/only.rs" in capsys.readouterr().out


def test_aggregate_cli_metadata_error_uses_parser(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Malformed evidence exits with argparse's input error status."""
    monkeypatch.setattr(sys, "argv", ["report_coverage", str(tmp_path)])
    with pytest.raises(SystemExit) as error:
        report_coverage.main()
    assert error.value.code == PARSER_ERROR


@pytest.mark.parametrize(
    ("exports", "expected"),
    [
        (
            'export RUSTFLAGS="-C instrument-coverage"\n\nexport EMPTY=;\n',
            {"RUSTFLAGS": "-C instrument-coverage", "EMPTY": ""},
        ),
        (
            "LLVM_PROFILE_FILE='/coverage/a b/%p.profraw'",
            {"LLVM_PROFILE_FILE": "/coverage/a b/%p.profraw"},
        ),
    ],
)
def test_shell_exports_are_decoded(exports: str, expected: dict[str, str]) -> None:
    """Quoted values retain spaces without invoking a shell."""
    assert run_coverage.parse_exports(exports) == expected


@pytest.mark.parametrize(
    "exports", ["export X=1 ; echo secret", "not-an-assignment", "export 3BAD=x"]
)
def test_malformed_shell_exports_are_rejected(exports: str) -> None:
    """Unexpected shell statements are not accepted as instrumentation settings."""
    with pytest.raises(ValueError, match="invalid"):
        run_coverage.parse_exports(exports)


class Collector:
    """Capture orchestration while writing deterministic report fixtures."""

    def __init__(self, output: Path, values: list[str]) -> None:
        """Store the selected report destination and command responses."""
        self.output = output
        output.mkdir()
        (output / "revision.txt").write_text("stale", encoding="utf-8")
        (output / "native.json").write_text("stale", encoding="utf-8")
        (output / "unrelated.txt").write_text("preserve", encoding="utf-8")
        self.values: Iterator[str] = iter(values)
        self.calls: list[tuple[list[str], Path, dict[str, str]]] = []

    def check_output(self, *_args: object, **_kwargs: object) -> str:
        """Return one response in fixed subprocess call order."""
        return next(self.values)

    def run(self, arguments: list[str], *, cwd: Path, env: dict[str, str], check: bool) -> None:
        """Record commands without recursively starting pytest or cargo."""
        assert check
        self.calls.append((arguments, cwd, env.copy()))
        (self.output / "rust.json").write_text(
            json.dumps({"data": [{"totals": {}}]}), encoding="utf-8"
        )
        (self.output / "python.json").write_text(json.dumps({"totals": {}}), encoding="utf-8")


@pytest.mark.parametrize("platform", ["linux", "win32"])
def test_collector_orchestrates_instrumented_commands(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, platform: str
) -> None:
    """The actual CLI tests use the instrumented binary and chosen venv path."""
    output = tmp_path / "coverage"
    python = tmp_path / "venv/bin/python"
    collector = Collector(
        output, [SHA, "", 'export LLVM_PROFILE_FILE="/coverage/profile %p"', SHA, "3.14"]
    )
    monkeypatch.setattr(sys, "platform", platform)
    monkeypatch.setattr(
        sys, "argv", ["run_coverage", "--python", str(python), "--output", str(output)]
    )
    monkeypatch.setattr(subprocess, "check_output", collector.check_output)
    monkeypatch.setattr(subprocess, "run", collector.run)
    run_coverage.main()
    command, cwd, environment = collector.calls[3]
    assert command[:3] == [str(python.absolute()), "-m", "pytest"]
    assert cwd == run_coverage.ROOT / "python"
    assert environment["LLVM_PROFILE_FILE"] == "/coverage/profile %p"
    assert (
        Path(environment["KIRYL_BINARY"]).name == {"linux": "kiryl", "win32": "kiryl.exe"}[platform]
    )
    assert collector.calls[0][0] == ["cargo", "llvm-cov", "clean", "--workspace"]
    assert collector.calls[-1][0][-3:] == ["--lcov", "--output-path", str(output / "rust.lcov")]
    assert json.loads((output / "native.json").read_text(encoding="utf-8")) == {
        "platform": platform,
        "python": "3.14",
        "dirty": False,
        "source_hashes": run_coverage.source_snapshot(run_coverage.ROOT),
    }
    assert (output / "revision.txt").read_text(encoding="utf-8").strip() == SHA


def test_collector_rejects_revision_change(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """A checkout change cannot receive successful revision metadata."""
    output = tmp_path / "coverage"
    collector = Collector(output, [SHA, "", "export RUSTFLAGS=x", "b" * 40])
    monkeypatch.setattr(sys, "argv", ["run_coverage", "--output", str(output)])
    monkeypatch.setattr(subprocess, "check_output", collector.check_output)
    monkeypatch.setattr(subprocess, "run", collector.run)
    with pytest.raises(SystemExit, match="revision changed"):
        run_coverage.main()
    assert not (output / "revision.txt").exists()
    assert not (output / "native.json").exists()
    assert (output / "unrelated.txt").read_text(encoding="utf-8") == "preserve"


def test_collector_command_failure_propagates(monkeypatch: pytest.MonkeyPatch) -> None:
    """A failing instrumented command must stop the collection pipeline."""
    monkeypatch.setattr(
        subprocess, "run", MagicMock(side_effect=subprocess.CalledProcessError(7, ["cargo"]))
    )
    with pytest.raises(subprocess.CalledProcessError) as error:
        run_coverage.execute(["cargo"], environment={})
    assert error.value.returncode == COMMAND_FAILURE


@pytest.mark.parametrize("script", ["report_coverage.py", "run_coverage.py"])
def test_script_entrypoint_invokes_cli(script: str, monkeypatch: pytest.MonkeyPatch) -> None:
    """Direct script invocation reaches argument parsing."""
    monkeypatch.syspath_prepend(str(run_coverage.ROOT / "tools"))
    monkeypatch.setattr(sys, "argv", [script, "--help"])
    with pytest.raises(SystemExit) as error:
        runpy.run_path(str(run_coverage.ROOT / "tools" / script), run_name="__main__")
    assert error.value.code == 0


@pytest.mark.parametrize("suffix", ["", "SF:src/other.rs\n"])
def test_undefined_function_at_boundary_is_rejected(tmp_path: Path, suffix: str) -> None:
    """Missing final delimiters cannot discard undefined function records."""
    report = tmp_path / "rust.lcov"
    report.write_text("SF:src/main.rs\nFNDA:1,unknown\n" + suffix, encoding="utf-8")
    with pytest.raises(ValueError, match="lack definitions"):
        report_coverage.rust_counts([report])


@pytest.mark.parametrize("record", ["DA:1,-1", "FN:1,main\nFNDA:-1,main", "FNDA:-1,main"])
def test_negative_hits_cannot_satisfy_gate(tmp_path: Path, record: str) -> None:
    """Malformed negative counts cannot be mistaken for covered code."""
    report = tmp_path / "rust.lcov"
    report.write_text("SF:src/main.rs\n" + record + "\nend_of_record\n", encoding="utf-8")
    with pytest.raises(ValueError, match="cannot be negative"):
        report_coverage.rust_counts([report])


def test_lcov_summary_tags_do_not_change_locations(tmp_path: Path) -> None:
    """Record summaries are not additional source locations."""
    report = tmp_path / "rust.lcov"
    report.write_text("TN:unit\n" + LCOV + "LF:1\nFNF:1\n", encoding="utf-8")
    assert report_coverage.rust_counts([report]) == {
        "src/main.rs": {"lines": {"1": 1}, "functions": {"1:main": 1}},
    }


@pytest.mark.parametrize(
    "name", ["src/main.rs", "python/tests/test_case.py", "tools/run_coverage.py"]
)
@pytest.mark.usefixtures("matrix")
def test_aggregate_inventory_rejects_entirely_missing_file(name: str) -> None:
    """A matrix cannot claim completeness after omitting an entire source file."""
    rust = {"src/main.rs": {"lines": {"1": 1}, "functions": {"main": 1}}}
    python = {
        "python/tests/test_case.py": {"lines": {"1": 1}, "branches": {}},
        "tools/run_coverage.py": {"lines": {"1": 1}, "branches": {}},
        "tools/report_coverage.py": {"lines": {"1": 1}, "branches": {}},
    }
    rust.pop(name, None)
    python.pop(name, None)
    with pytest.raises(ValueError, match="source files omitted"):
        report_coverage.validate_inventory(rust, python)


@pytest.mark.parametrize("dirty", [None, "false"])
def test_native_metadata_requires_dirty_boolean(matrix: Path, dirty: object) -> None:
    """Unrecorded working tree state cannot masquerade as a clean revision."""
    path = matrix / "linux-3.14/native.json"
    metadata = json.loads(path.read_text(encoding="utf-8"))
    metadata["dirty"] = dirty
    path.write_text(json.dumps(metadata), encoding="utf-8")
    with pytest.raises(ValueError, match="source snapshot"):
        report_coverage.report_directories(matrix)


def test_native_metadata_rejects_inconsistent_snapshot(matrix: Path) -> None:
    """Matching commit IDs cannot conceal different working-tree bytes."""
    path = matrix / "linux-3.14/native.json"
    metadata = json.loads(path.read_text(encoding="utf-8"))
    metadata["source_hashes"].pop("src/main.rs")
    path.write_text(json.dumps(metadata), encoding="utf-8")
    with pytest.raises(ValueError, match="source snapshot"):
        report_coverage.report_directories(matrix)


@pytest.mark.usefixtures("matrix")
def test_executable_library_source_must_appear_in_report() -> None:
    """The module-only exception stops applying when the library gains code."""
    (report_coverage.ROOT / "src/lib.rs").write_text("fn added() {}\n", encoding="utf-8")
    with pytest.raises(ValueError, match=r"src/lib\.rs"):
        report_coverage.validate_inventory({}, {})


@pytest.mark.usefixtures("matrix")
def test_missing_library_file_does_not_create_inventory_exception() -> None:
    """Source inventory remains usable without a declaration-only library."""
    (report_coverage.ROOT / "src/lib.rs").unlink()
    with pytest.raises(ValueError, match="source files omitted"):
        report_coverage.validate_inventory({}, {})


def test_snapshot_normalizes_native_line_endings(tmp_path: Path) -> None:
    """Windows checkout line endings preserve the same source snapshot."""
    (tmp_path / "src").mkdir()
    source = tmp_path / "src/main.rs"
    source.write_bytes(b"fn main() {}\r\n")
    windows = run_coverage.source_snapshot(tmp_path)
    source.write_bytes(b"fn main() {}\n")
    assert run_coverage.source_snapshot(tmp_path) == windows


def test_collector_rejects_dirty_source_mutation(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Uncommitted edits during collection invalidate the report metadata."""
    output = tmp_path / "coverage"
    collector = Collector(output, [SHA, " M src/main.rs", "export RUSTFLAGS=x", SHA])
    monkeypatch.setattr(sys, "argv", ["run_coverage", "--output", str(output)])
    monkeypatch.setattr(subprocess, "check_output", collector.check_output)
    monkeypatch.setattr(subprocess, "run", collector.run)
    monkeypatch.setattr(
        run_coverage,
        "source_snapshot",
        MagicMock(side_effect=[{"source": "before"}, {"source": "after"}]),
    )
    with pytest.raises(SystemExit, match="revision changed"):
        run_coverage.main()
    assert not (output / "native.json").exists()
