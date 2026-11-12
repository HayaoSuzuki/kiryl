# Copyright (c) 2026 kiryl contributors
"""Discover supported pytest invocations; stdout contains exactly one JSON value.

Run in the selected target interpreter with pytest selection arguments. Imports
and collection may execute user code; this command is not a sandbox.
"""

import hashlib
import inspect
import json
import os
import sys
from pathlib import Path
from typing import TYPE_CHECKING, Any, cast

if TYPE_CHECKING:
    import pytest


def source_hashes(root: Path | None) -> dict[str, str]:
    """Snapshot Python source/configuration under the collected pytest root.

    Symlinks and generated environments are excluded; external files and data
    remain explicit reproduction assumptions.
    """
    excluded = {".git", ".venv", ".kiryl", "__pycache__", "target", "site", ".hypothesis"}
    sources = {}
    if root is not None:
        for directory, dirs, names in os.walk(root):
            dirs[:] = sorted(
                d for d in dirs if d not in excluded and not Path(directory, d).is_symlink()
            )
            for name in sorted(names):
                path = Path(directory, name)
                if path.is_symlink():
                    continue
                if path.suffix == ".py" or name in {
                    "pyproject.toml",
                    "pytest.ini",
                    "tox.ini",
                    "setup.cfg",
                    "uv.lock",
                    "requirements.txt",
                }:
                    sources[path.relative_to(root).as_posix()] = hashlib.sha256(
                        path.read_bytes()
                    ).hexdigest()
    return sources


class _Selection:
    """Read final pytest-selected nodes without setting up fixtures."""

    def __init__(self) -> None:
        """Hold discovery results for one collection session."""
        self.nodeids: list[str] = []
        self.root: Path | None = None

    def pytest_configure(self, config: pytest.Config) -> None:
        """Record pytest root for source/configuration identity."""
        self.root = config.rootpath.resolve()

    def pytest_collection_finish(self, session: pytest.Session) -> None:
        """Inspect only final selection after pytest deselection hooks."""
        import hypothesis  # noqa: PLC0415 -- runtime admitted before pytest
        import pytest  # noqa: PLC0415
        from hypothesis.stateful import get_state_machine_test  # noqa: PLC0415

        for item in session.items:
            if not isinstance(item, pytest.Function):
                continue
            wrapped = item.obj
            factory = getattr(wrapped, "_hypothesis_state_machine_class", None)
            if factory is not None:
                wrapped = get_state_machine_test(factory)
            if not hypothesis.is_hypothesis_test(wrapped):
                continue
            inner = cast("Any", wrapped).hypothesis.inner_test
            if inspect.iscoroutinefunction(inner) or inspect.isasyncgenfunction(inner):
                continue
            self.nodeids.append(item.nodeid)


def main() -> None:
    """Reserve JSON output before pytest or target imports can write to fd 1."""
    output = os.fdopen(os.dup(sys.stdout.fileno()), "w", encoding="utf-8")
    os.dup2(sys.stderr.fileno(), sys.stdout.fileno())
    from runtime import validate_runtime  # noqa: PLC0415 -- reserve stdout first

    try:
        validate_runtime()
    except RuntimeError as error:
        json.dump({"version": 1, "error": str(error)}, output)
        output.write("\n")
        output.close()
        raise SystemExit(2) from error
    import hypothesis  # noqa: PLC0415 -- reserve stdout before dependencies
    import pytest  # noqa: PLC0415 -- reserve stdout before dependencies

    selection = _Selection()
    code = int(pytest.main([*sys.argv[1:], "--collect-only", "-s"], plugins=[selection]))
    sources = source_hashes(selection.root)
    value = {
        "version": 1,
        "nodeids": selection.nodeids,
        "root": str(selection.root),
        "sources": sources,
        "implementation_sources": {
            str(path.relative_to(Path(__file__).resolve().parents[2])): hashlib.sha256(
                path.read_bytes()
            ).hexdigest()
            for path in sorted(
                [
                    *Path(__file__).resolve().parent.glob("*.py"),
                    *Path(__file__).resolve().parents[2].joinpath("src").rglob("*.rs"),
                    Path(__file__).resolve().parents[2] / "Cargo.lock",
                ]
            )
        },
        "selection": sys.argv[1:],
        "environment": {
            "platform": sys.platform,
            "python": list(sys.version_info[:2]),
            "implementation": sys.implementation.name,
            "hypothesis": hypothesis.__version__,
            "pytest": pytest.__version__,
            "executable": sys.executable,
        },
    }
    if code not in (0, 5):
        value["error"] = f"pytest collection exit: {code}"
    json.dump(value, output)
    output.write("\n")
    output.close()
    raise SystemExit(0 if code in (0, 5) else code)


if __name__ == "__main__":
    main()
