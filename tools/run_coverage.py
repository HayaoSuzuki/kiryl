# Copyright (c) 2026 kiryl contributors
"""Collect native Rust and Python coverage without hiding test source files."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shlex
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
IGNORE_BUILD = r"[/\\](\.cargo|\.rustup)[/\\]|[/\\]rustc[/\\]|[/\\]target[/\\]"


def execute(arguments: list[str], *, environment: dict[str, str], cwd: Path = ROOT) -> None:
    """Fail immediately when an instrumented build or test command fails."""
    subprocess.run(arguments, cwd=cwd, env=environment, check=True)  # noqa: S603 - explicit argv


def parse_exports(exports: str) -> dict[str, str]:
    """Decode cargo-llvm-cov assignments without executing shell code."""
    result: dict[str, str] = {}
    for line in exports.splitlines():
        if not line.strip():
            continue
        parts = shlex.split(line.removeprefix("export ").rstrip(";"))
        if len(parts) != 1 or "=" not in parts[0]:
            msg = f"invalid cargo-llvm-cov environment assignment: {line}"
            raise ValueError(msg)
        name, value = parts[0].split("=", 1)
        if not name.isidentifier():
            msg = f"invalid environment variable name: {name}"
            raise ValueError(msg)
        result[name] = value
    return result


def source_snapshot(root: Path) -> dict[str, str]:
    """Hash maintained execution inputs with platform-neutral LF line endings."""
    inputs = set()
    for directory, pattern in (
        ("src", "*.rs"),
        ("tests", "*.rs"),
        ("python/kiryl_adapter", "*.py"),
        ("python/tests", "*.py"),
        ("python/specimens", "*.py"),
        ("formal", "*.tsv"),
    ):
        inputs.update((root / directory).rglob(pattern))
    for name in (
        "tools/run_coverage.py",
        "tools/report_coverage.py",
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "python/pyproject.toml",
        "python/uv.lock",
    ):
        path = root / name
        if path.is_file():
            inputs.add(path)
    return {
        path.relative_to(root).as_posix(): hashlib.sha256(
            path.read_bytes().replace(b"\r\n", b"\n"),
        ).hexdigest()
        for path in sorted(inputs)
    }


def revision() -> str:
    """Read the full checkout commit for native report pairing."""
    return subprocess.check_output(
        ["git", "rev-parse", "HEAD"],  # noqa: S607 - developer tool
        cwd=ROOT,
        text=True,
    ).strip()


def main() -> None:
    """Collect evidence; the combined report enforces the 100 percent contract."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--python",
        type=Path,
        default=ROOT
        / "python/.venv"
        / ("Scripts/python.exe" if sys.platform == "win32" else "bin/python"),
    )
    parser.add_argument("--output", type=Path, default=ROOT / "product-evidence/coverage")
    arguments = parser.parse_args()
    original_revision = revision()
    original_sources = source_snapshot(ROOT)
    dirty = bool(
        subprocess.check_output(
            ["git", "status", "--porcelain", "--untracked-files=normal"],  # noqa: S607 - developer tool
            cwd=ROOT,
            text=True,
        ).strip()
    )
    output = arguments.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    # A failed rerun must not retain successful metadata from earlier evidence.
    (output / "revision.txt").unlink(missing_ok=True)
    (output / "native.json").unlink(missing_ok=True)
    environment = dict(os.environ)
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target/coverage")
    exports = subprocess.check_output(
        ["cargo", "llvm-cov", "show-env", "--sh"],  # noqa: S607 - developer tool
        cwd=ROOT,
        env=environment,
        text=True,
    )
    environment.update(parse_exports(exports))
    execute(["cargo", "llvm-cov", "clean", "--workspace"], environment=environment)
    execute(["cargo", "test", "--locked", "--workspace", "--all-features"], environment=environment)
    execute(["cargo", "build", "--locked"], environment=environment)
    environment["KIRYL_BINARY"] = str(
        ROOT / "target/coverage/debug" / ("kiryl.exe" if sys.platform == "win32" else "kiryl")
    )
    environment["COVERAGE_FILE"] = str(output / ".coverage")
    execute(
        [
            str(arguments.python.absolute()),
            "-m",
            "pytest",
            "--cov",
            "--cov-fail-under=0",
            "--cov-report=term-missing",
            f"--cov-report=json:{output / 'python.json'}",
            f"--junitxml={output / 'tests.xml'}",
            "--randomly-seed=17",
        ],
        environment=environment,
        cwd=ROOT / "python",
    )
    # Disable cargo-llvm-cov's default test exclusion; retain only build exclusions.
    for format_name in ("json", "lcov"):
        execute(
            [
                "cargo",
                "llvm-cov",
                "report",
                "--no-default-ignore-filename-regex",
                "--ignore-filename-regex",
                IGNORE_BUILD,
                f"--{format_name}",
                "--output-path",
                str(output / f"rust.{format_name}"),
            ],
            environment=environment,
        )
    if revision() != original_revision or source_snapshot(ROOT) != original_sources:
        message = "source revision changed during coverage collection"
        raise SystemExit(message)
    (output / "revision.txt").write_text(original_revision + "\n", encoding="utf-8")
    version = subprocess.check_output(  # noqa: S603 - explicitly selected interpreter
        [
            str(arguments.python.absolute()),
            "-c",
            "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')",
        ],
        text=True,
    ).strip()
    (output / "native.json").write_text(
        json.dumps(
            {
                "platform": sys.platform,
                "python": version,
                "dirty": dirty,
                "source_hashes": original_sources,
            }
        ),
        encoding="utf-8",
    )
    totals = json.loads((output / "rust.json").read_text(encoding="utf-8"))["data"][0]["totals"]
    python_totals = json.loads((output / "python.json").read_text(encoding="utf-8"))["totals"]
    print(json.dumps({"rust": totals, "python": python_totals}, indent=2))  # noqa: T201 - CLI evidence


if __name__ == "__main__":
    main()
