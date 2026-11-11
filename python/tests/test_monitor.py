# Copyright (c) 2026 kiryl contributors
"""Per-case coverage attribution and coexistence with independent monitoring."""

import importlib.util
import json
import os
import subprocess
import sys
from pathlib import Path
from types import FunctionType, ModuleType
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from types import CodeType

import pytest
from adapter import Invocation, replay, run_session
from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st
from monitor import BranchMonitor


@pytest.fixture
def branches(tmp_path: Path) -> ModuleType:
    """Load an independently authored two-branch module."""
    path = tmp_path / "branches.py"
    path.write_text("def classify(n):\n    if n > 0:\n        return 1\n    return 0\n")
    spec = importlib.util.spec_from_file_location("probe_branches", path)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_repeated_cases_have_independent_features(branches: ModuleType, tmp_path: Path) -> None:
    """Keep left/right novelty without losing repeated observations on later cases."""
    monitor = BranchMonitor((tmp_path,))
    try:
        features = []
        for value in [1, -1, 1]:
            monitor.start_case()
            assert branches.classify(value) in (0, 1)
            features.append(monitor.finish_case())
        left, right, repeated_left = features
        assert left
        assert right
        assert set(left) != set(right)
        assert repeated_left == left
        assert all(feature.startswith("specimen:") for feature in left + right)
    finally:
        monitor.close()


def test_other_tools_keep_disabled_events(branches: ModuleType, tmp_path: Path) -> None:
    """Never globally restart another client's deliberately disabled callback."""
    tool = next(index for index in range(6) if sys.monitoring.get_tool(index) is None)
    sys.monitoring.use_tool_id(tool, "other-probe")
    seen: list[int] = []

    def callback(code: CodeType, offset: int, destination: int) -> object:
        del offset, destination
        if code.co_name == "classify":
            seen.append(1)
        return sys.monitoring.DISABLE

    events = sys.monitoring.events.BRANCH_LEFT | sys.monitoring.events.BRANCH_RIGHT
    sys.monitoring.register_callback(tool, sys.monitoring.events.BRANCH_LEFT, callback)
    sys.monitoring.register_callback(tool, sys.monitoring.events.BRANCH_RIGHT, callback)
    sys.monitoring.set_events(tool, events)
    monitor = BranchMonitor((tmp_path,))
    try:
        monitor.start_case()
        branches.classify(1)
        monitor.finish_case()
        first = len(seen)
        monitor.start_case()
        branches.classify(1)
        monitor.finish_case()
        assert len(seen) == first
        assert sys.monitoring.get_tool(tool) == "other-probe"
        monitor.start_case()
        branches.classify(-1)
        monitor.finish_case()
        assert len(seen) > first
    finally:
        monitor.close()
        sys.monitoring.free_tool_id(tool)


def test_no_tool_slot_is_infrastructure(tmp_path: Path) -> None:
    """Refuse a slot conflict without taking ownership of another tool's events."""
    owned = []
    try:
        for index in range(6):
            if sys.monitoring.get_tool(index) is None:
                sys.monitoring.use_tool_id(index, "occupied-probe")
                owned.append(index)
        with pytest.raises(RuntimeError, match="monitoring"):
            BranchMonitor((tmp_path,))
        assert all(sys.monitoring.get_tool(index) == "occupied-probe" for index in owned)
    finally:
        for index in owned:
            sys.monitoring.free_tool_id(index)


def test_outside_roots_never_supply_features(tmp_path: Path) -> None:
    """Exclude test framework/generator code rather than rewarding harness paths."""
    monitor = BranchMonitor((tmp_path,))
    try:
        monitor.start_case()
        for value in range(10):
            assert value >= 0
        assert monitor.finish_case() == ()
    finally:
        monitor.close()


def test_adapter_reports_only_application_edges(tmp_path: Path) -> None:
    """Exclude collected test/oracle branches while returning actual app feedback."""
    (tmp_path / "pytest.ini").write_text("[pytest]\n")
    (tmp_path / "app_probe.py").write_text(
        "def classify(n):\n    if n:\n        return 1\n    return 0\n"
    )
    target = tmp_path / "test_app_probe.py"
    target.write_text("""
from hypothesis import given, strategies as st
from app_probe import classify
@given(n=st.integers(min_value=0, max_value=1))
def test_property(n):
    assert classify(n) == n
""")
    try:
        evidence = run_session(
            [str(target), "-q"],
            [Invocation("test_app_probe.py::test_property", (b"\x00", b"\x01", b"\x00"))],
        )
        left, right, again = [case.features for case in evidence.cases]
        assert evidence.coverage_roots == (str(tmp_path.resolve()),)
        assert left
        assert right
        assert left == again
        assert set(left) != set(right)
        assert all(
            "app_probe.py:" in feature and "test_app_probe" not in feature
            for feature in left + right
        )
        disabled = run_session(
            [str(target), "-q"],
            [Invocation("test_app_probe.py::test_property", (b"\x00", b"\x01", b"\x00"))],
            collect_coverage=False,
        )
        assert [case.kind for case in disabled.cases] == [case.kind for case in evidence.cases]
        assert all(case.features == () for case in disabled.cases)
        assert disabled.coverage_roots == ()
    finally:
        sys.modules.pop("test_app_probe", None)
        sys.modules.pop("app_probe", None)


def test_feature_ids_match_an_independent_interpreter(branches: ModuleType, tmp_path: Path) -> None:
    """Make shared corpus novelty independent of code-object addresses/refcounts."""
    monitor = BranchMonitor((tmp_path,))
    try:
        monitor.start_case()
        branches.classify(1)
        expected = monitor.finish_case()
    finally:
        monitor.close()
    script = """
import importlib.util, json, sys
from pathlib import Path
from monitor import BranchMonitor
root = Path(sys.argv[1])
spec = importlib.util.spec_from_file_location("other_name", root / "branches.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
monitor = BranchMonitor((root,))
monitor.start_case()
module.classify(1)
print(json.dumps(monitor.finish_case()))
monitor.close()
"""
    completed = subprocess.run(  # noqa: S603 -- owned bootstrap and specimen
        [sys.executable, "-c", script, str(tmp_path)],
        cwd=Path(__file__).parents[1] / "kiryl_adapter",
        capture_output=True,
        text=True,
        timeout=10,
        check=True,
    )
    assert tuple(json.loads(completed.stdout)) == expected


def test_missing_source_is_coverage_error_not_target_error(
    branches: ModuleType,
    tmp_path: Path,
) -> None:
    """Defer monitoring IO failures until the case ends, leaving target behavior intact."""
    (tmp_path / "branches.py").unlink()
    monitor = BranchMonitor((tmp_path,))
    try:
        monitor.start_case()
        assert branches.classify(1) == 1
        with pytest.raises(RuntimeError, match="monitoring source"):
            monitor.finish_case()
    finally:
        monitor.close()


@pytest.mark.usefixtures("branches")
def test_invalid_code_filename_does_not_inject_a_target_exception(tmp_path: Path) -> None:
    """Keep code-path validation errors out of the target's exception behavior."""
    # coverage.py itself rejects NUL filenames before our callback runs. Isolate
    # this owned-code boundary probe instead of disabling another client's hooks.
    script = r"""
import importlib.util, sys
from pathlib import Path
from monitor import BranchMonitor
root = Path(sys.argv[1])
spec = importlib.util.spec_from_file_location("nul_probe", root / "branches.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
module.classify.__code__ = module.classify.__code__.replace(
    co_filename=str(root / "invalid\x00.py")
)
monitor = BranchMonitor((root,))
try:
    monitor.start_case()
    assert module.classify(1) == 1
    try:
        monitor.finish_case()
    except RuntimeError as error:
        assert "monitoring source" in str(error)
    else:
        raise AssertionError("missing coverage infrastructure error")
finally:
    monitor.close()
"""
    environment = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith(("COV_CORE_", "COVERAGE_"))
    }
    subprocess.run(  # noqa: S603 -- owned boundary probe, without upstream coverage hooks
        [sys.executable, "-c", script, str(tmp_path)],
        cwd=Path(__file__).parents[1] / "kiryl_adapter",
        env=environment,
        capture_output=True,
        timeout=10,
        check=True,
    )


def test_close_cannot_free_a_slot_reused_by_another_tool(tmp_path: Path) -> None:
    """Keep close idempotent even after another client reuses its released slot."""
    before = [sys.monitoring.get_tool(index) for index in range(6)]
    monitor = BranchMonitor((tmp_path,))
    slot = next(
        index
        for index in range(6)
        if before[index] is None and sys.monitoring.get_tool(index) == "kiryl-branch-probe"
    )
    monitor.close()
    sys.monitoring.use_tool_id(slot, "replacement")
    try:
        monitor.close()
        assert sys.monitoring.get_tool(slot) == "replacement"
    finally:
        sys.monitoring.free_tool_id(slot)


def test_generated_pseudo_filenames_are_excluded_not_read_as_files(branches: ModuleType) -> None:
    """Ignore generated dataclass/exec code without inventing filesystem failures."""
    original = branches.classify.__code__
    branches.classify.__code__ = original.replace(co_filename="<string>")
    monitor = BranchMonitor((Path.cwd(),), excluded=(Path(__file__),))
    try:
        monitor.start_case()
        assert branches.classify(1) == 1
        assert monitor.finish_case() == ()
    finally:
        monitor.close()
        branches.classify.__code__ = original


def test_identical_code_in_distinct_files_has_distinct_features(
    branches: ModuleType,
    tmp_path: Path,
) -> None:
    """Do not let structural CodeType equality merge different source owners."""
    copied_path = tmp_path / "copied.py"
    copied_path.write_text((tmp_path / "branches.py").read_text())
    copied = FunctionType(
        branches.classify.__code__.replace(co_filename=str(copied_path)), vars(branches)
    )
    monitor = BranchMonitor((tmp_path,))
    try:
        monitor.start_case()
        branches.classify(1)
        first = monitor.finish_case()
        monitor.start_case()
        copied(1)
        second = monitor.finish_case()
        assert first
        assert second
        assert set(first).isdisjoint(second)
        assert all(":copied.py:" in feature for feature in second)
    finally:
        monitor.close()


@settings(
    max_examples=20, deadline=None, suppress_health_check=[HealthCheck.function_scoped_fixture]
)
@given(values=st.lists(st.booleans(), min_size=1, max_size=20))
def test_arbitrary_case_sequences_keep_only_their_own_edges(
    branches: ModuleType,
    tmp_path: Path,
    values: list[bool],
) -> None:
    """Exercise reset/attribution under arbitrary repeated true/false input order."""
    monitor = BranchMonitor((tmp_path,))
    known: dict[bool, tuple[str, ...]] = {}
    try:
        for value in [True, False, *values]:
            monitor.start_case()
            branches.classify(int(value))
            features = monitor.finish_case()
            if value in known:
                assert features == known[value]
            known[value] = features
        assert known[True]
        assert known[False]
        assert known[True] != known[False]
    finally:
        monitor.close()


def test_fresh_replay_preserves_adapter_identity_for_fixture_stash(scopes: Path) -> None:
    """Keep one adapter module identity when a fixture imports its public stash key."""
    node = "test_scopes.py::test_fixture_failure"
    args = [str(scopes), "-q"]
    evidence = run_session(args, [Invocation(node, (b"",))])
    assert evidence.cases[0].kind == "failure"
    token = evidence.cases[0].replay_token
    assert token is not None
    checked = replay(node, token, args)
    assert checked.kind == "failure", checked.rendered
    assert checked.verification == "confirmed"
