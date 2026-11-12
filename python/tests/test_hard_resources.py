# Copyright (c) 2026 kiryl contributors
"""Exercise native aggregate-memory enforcement through the instrumented CLI."""

from __future__ import annotations

import importlib.util
import os
import sys
from pathlib import Path
from typing import TYPE_CHECKING, Protocol, cast

import pytest
from test_cli import BINARY

if TYPE_CHECKING:
    from typing import Any


class HardResourceRunner(Protocol):
    """Typed boundary for the unchanged historical native acceptance driver."""

    TARGET: str

    def check_run(  # noqa: PLR0913 -- unchanged legacy driver has six named inputs
        self,
        *,
        binary: Path,
        python: Path,
        root: Path,
        evidence: Path,
        name: str,
        memory: int,
    ) -> dict[str, Any]:
        """Execute and verify exactly one native memory-control campaign."""
        ...


pytestmark = [
    pytest.mark.skipif(not BINARY.exists(), reason="cargo build required"),
    pytest.mark.skipif(
        sys.platform not in {"linux", "win32"}
        or (sys.platform == "linux" and not bool(os.environ.get("KIRYL_CGROUP_PARENT"))),
        reason="requires native Windows jobs or an explicitly delegated Linux cgroup",
    ),
]


@pytest.fixture
def hard_resource_runner() -> HardResourceRunner:
    """Load the existing gate without invoking its two-campaign main function."""
    path = (
        Path(__file__).parents[2]
        / "docs/development/work/product-cli/evidence/hard-resource-run.py"
    )
    spec = importlib.util.spec_from_file_location("kiryl_hard_resource_evidence", path)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return cast("HardResourceRunner", module)


@pytest.fixture
def hard_resource_project(
    tmp_path: Path, hard_resource_runner: HardResourceRunner
) -> tuple[Path, Path]:
    """Keep target files, traces and evidence inside one case's owned directory."""
    project = tmp_path / "project"
    project.mkdir()
    (project / "pytest.ini").write_text("[pytest]\n", encoding="utf-8")
    (project / "test_hard.py").write_text(hard_resource_runner.TARGET, encoding="utf-8")
    evidence = tmp_path / "evidence"
    evidence.mkdir()
    return project, evidence


@pytest.mark.parametrize(("name", "memory_mib"), [("control", 768), ("aggregate", 256)])
def test_native_aggregate_memory_campaign(
    name: str,
    memory_mib: int,
    hard_resource_runner: HardResourceRunner,
    hard_resource_project: tuple[Path, Path],
) -> None:
    """Verify one allowance or aggregate-rejection campaign and its native backend."""
    project, evidence = hard_resource_project
    report = hard_resource_runner.check_run(
        binary=BINARY,
        python=Path(sys.executable),
        root=project,
        evidence=evidence,
        name=name,
        memory=memory_mib,
    )
    capabilities = report["targets"][0]["report"]["capabilities"]
    assert capabilities["tree_memory_hard"] is True
    assert capabilities["backend"] == {"linux": "cgroup-v2", "win32": "job-object"}[sys.platform]
