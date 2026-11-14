# Copyright (c) 2026 kiryl contributors
"""Product discovery is machine readable despite arbitrary target output."""

import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path

import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

COLLECT = Path(__file__).parents[1] / "kiryl_adapter" / "collect.py"


@pytest.fixture
def collection_project(tmp_path: Path) -> Path:
    """Create synchronous, asynchronous, parameterized and stateful collection targets."""
    (tmp_path / "pytest.ini").write_text("[pytest]\n")
    (tmp_path / "test_targets.py").write_text("""
import os
os.write(1, b"target output\\n")
import pytest
from hypothesis import given, strategies as st
from hypothesis.stateful import RuleBasedStateMachine, rule
@given(st.integers())
def test_property(n): pass
@pytest.mark.parametrize("tag", ["a", "b"])
@given(n=st.integers())
def test_parameter(tag, n): pass
@given(st.integers())
async def test_async(n): pass
def test_fixed(): pass
class Machine(RuleBasedStateMachine):
    @rule()
    def step(self): pass
TestMachine = Machine.TestCase
""")
    return tmp_path


def test_collect_sync_stateful_and_parameters(collection_project: Path) -> None:
    """Exclude unsupported functions and retain real parametrized node IDs."""
    tmp_path = collection_project
    result = subprocess.run(  # noqa: S603 -- owned interpreter and target
        [sys.executable, str(COLLECT), str(tmp_path)], capture_output=True, check=False
    )
    assert result.returncode == 0, result.stderr
    data = json.loads(result.stdout)
    assert len(data["nodeids"]) == 4  # noqa: PLR2004 -- property, two variants, state machine
    assert "TestMachine" in "\n".join(data["nodeids"])
    assert "async" not in "\n".join(data["nodeids"])
    assert "fixed" not in "\n".join(data["nodeids"])
    assert data["environment"]["hypothesis"] == "6.170.13"
    assert b"target output" in result.stderr


def test_collection_honors_pytest_selection(collection_project: Path) -> None:
    """Selection limits discovery to the same supported property pytest selected."""
    tmp_path = collection_project
    selected = subprocess.run(  # noqa: S603 -- owned interpreter and specimen
        [sys.executable, str(COLLECT), str(tmp_path), "-k", "property"],
        capture_output=True,
        check=False,
    )
    assert selected.returncode == 0, selected.stderr
    assert json.loads(selected.stdout)["nodeids"] == ["test_targets.py::test_property"]


def test_collection_normalizes_symlinked_project_root(tmp_path: Path) -> None:
    """Aliases of one project retain its canonical source identity."""
    project = tmp_path / "project"
    project.mkdir()
    (project / "pytest.ini").write_text("[pytest]\n")
    (project / "test_target.py").write_text(
        "from hypothesis import given, strategies as st\n"
        "@given(st.just(0))\n"
        "def test_property(value):\n    assert value == 0\n"
    )
    alias = tmp_path / "alias"
    alias.symlink_to(project, target_is_directory=True)
    result = subprocess.run(  # noqa: S603 -- owned project and canonical-root boundary
        [sys.executable, str(COLLECT), str(alias), "--rootdir", str(alias)],
        capture_output=True,
        check=False,
        timeout=10,
    )
    assert result.returncode == 0, result.stderr
    data = json.loads(result.stdout)
    assert data["root"] == str(project.resolve())
    assert "test_target.py" in data["sources"]


def test_collection_error_is_json(tmp_path: Path) -> None:
    """Collection import failures return a structured infrastructure error."""
    (tmp_path / "test_bad.py").write_text("raise RuntimeError('broken import')")
    result = subprocess.run(  # noqa: S603 -- owned interpreter and target
        [sys.executable, str(COLLECT), str(tmp_path)], capture_output=True, check=False
    )
    assert result.returncode != 0
    assert json.loads(result.stdout)["error"]


def test_source_snapshot_changes_and_excludes_environment(tmp_path: Path) -> None:
    """Recorded hashes detect edits without tracking virtual environments."""
    from collect import source_hashes  # noqa: PLC0415 -- product-only module

    source = tmp_path / "app.py"
    source.write_text("value = 1\n")
    (tmp_path / ".venv").mkdir()
    (tmp_path / ".venv" / "hidden.py").write_text("ignored")
    before = source_hashes(tmp_path)
    assert set(before) == {"app.py"}
    source.write_text("value = 2\n")
    assert source_hashes(tmp_path) != before


@settings(max_examples=24, deadline=None)
@given(
    directories=st.lists(
        st.text(alphabet="abcxyz", min_size=1, max_size=6), min_size=1, max_size=3
    ),
    content=st.binary(max_size=64),
)
def test_source_snapshot_includes_nested_files(directories: list[str], content: bytes) -> None:
    """Source identity includes files at arbitrary bounded directory depths."""
    from collect import source_hashes  # noqa: PLC0415 -- product-only module

    with tempfile.TemporaryDirectory(prefix="kiryl-source-snapshot-") as directory:
        root = Path(directory)
        source = root.joinpath(*directories, "app.py")
        source.parent.mkdir(parents=True)
        source.write_bytes(content)
        assert source_hashes(root) == {
            source.relative_to(root).as_posix(): hashlib.sha256(content).hexdigest(),
        }


def test_unsupported_runtime_rejects_before_target_import(tmp_path: Path) -> None:
    """Incompatible dependencies cannot execute project collection hooks or tests."""
    marker = tmp_path / "imported.txt"
    (tmp_path / "test_target.py").write_text(
        f"from pathlib import Path\nPath({str(marker)!r}).write_text('imported')\n"
    )
    script = (
        "import hypothesis,sys;hypothesis.__version__='unsupported';"
        f"sys.path.insert(0,{str(COLLECT.parent)!r});"
        f"sys.argv=['collect',{str(tmp_path)!r}];import collect;collect.main()"
    )
    result = subprocess.run(  # noqa: S603 -- owned bootstrap and target
        [sys.executable, "-c", script], capture_output=True, check=False, timeout=10
    )
    assert result.returncode != 0
    assert not marker.exists()
    assert "unsupported" in json.loads(result.stdout)["error"]


def test_source_snapshot_without_root_is_empty() -> None:
    """An unconfigured collection root cannot invent project identity."""
    from collect import source_hashes  # noqa: PLC0415 -- product-only module

    assert source_hashes(None) == {}


@pytest.mark.parametrize("symlink_kind", ["file", "directory"])
def test_source_snapshot_excludes_symlinks(tmp_path: Path, symlink_kind: str) -> None:
    """Neither linked files nor linked directories belong to the source snapshot."""
    from collect import source_hashes  # noqa: PLC0415 -- product-only module

    source = tmp_path / "app.py"
    source.write_text("value = 1\n")
    targets = {"file": source, "directory": tmp_path}
    (tmp_path / "linked.py").symlink_to(targets[symlink_kind])
    assert set(source_hashes(tmp_path)) == {"app.py"}


def test_collection_ignores_non_function_items(tmp_path: Path) -> None:
    """A collected doctest is not a Hypothesis invocation."""
    (tmp_path / "pytest.ini").write_text("[pytest]\n")
    (tmp_path / "example.py").write_text('"""\n>>> 1 + 1\n2\n"""\n')
    result = subprocess.run(  # noqa: S603 -- owned interpreter and doctest project
        [sys.executable, str(COLLECT), str(tmp_path), "--doctest-modules"],
        capture_output=True,
        check=False,
    )
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout)["nodeids"] == []
