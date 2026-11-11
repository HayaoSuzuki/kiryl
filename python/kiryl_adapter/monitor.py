# Copyright (c) 2026 kiryl contributors
"""Own one monitoring slot and attribute only application branches per case."""

import hashlib
import sys
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from types import CodeType


class BranchMonitor:
    """Collect repeatable, environment-scoped edges without resetting other tools."""

    def __init__(self, roots: tuple[Path, ...], *, excluded: tuple[Path, ...] = ()) -> None:
        """Reserve a free tool slot and record explicit application scope."""
        self.roots = tuple(root.resolve() for root in roots)
        self.excluded = frozenset(path.resolve() for path in excluded) | {
            Path(__file__).resolve(),
            Path(__file__).with_name("adapter.py").resolve(),
            Path(__file__).with_name("hypothesis_bridge.py").resolve(),
        }
        self._active = False
        self._closed = False
        self._features: set[str] = set()
        self._identities: dict[int, tuple[CodeType, str | None]] = {}
        self._sources: dict[Path, str] = {}
        self._error: OSError | ValueError | None = None
        slot = None
        for candidate in range(6):
            try:
                sys.monitoring.use_tool_id(candidate, "kiryl-branch-probe")
            except ValueError:
                continue
            slot = candidate
            break
        if slot is None:
            message = "no free sys.monitoring tool slot"
            raise RuntimeError(message)
        self._slot = slot
        events = sys.monitoring.events
        try:
            sys.monitoring.register_callback(slot, events.BRANCH_LEFT, self._branch)
            sys.monitoring.register_callback(slot, events.BRANCH_RIGHT, self._branch)
            sys.monitoring.set_events(slot, events.BRANCH_LEFT | events.BRANCH_RIGHT)
        except BaseException:
            sys.monitoring.free_tool_id(slot)
            raise

    def _identity(self, code: CodeType) -> str | None:
        cached = self._identities.get(id(code))
        if cached is not None:
            return cached[1]
        if not code.co_filename or (
            code.co_filename.startswith("<") and code.co_filename.endswith(">")
        ):
            self._identities[id(code)] = (code, None)
            return None
        path = Path(code.co_filename).resolve()
        identity = None
        for index, root in enumerate(self.roots):
            if path in self.excluded or not path.is_relative_to(root):
                continue
            relative = path.relative_to(root)
            if set(relative.parts) & {".venv", "site-packages", "hypothesis", "_pytest", "pytest"}:
                continue
            if path not in self._sources:
                self._sources[path] = hashlib.sha256(path.read_bytes()).hexdigest()
            code_digest = hashlib.sha256(code.co_code).hexdigest()
            identity = (
                f"specimen:{sys.version_info[:2]}:{index}:{relative}:"
                f"{self._sources[path]}:{code.co_qualname}:{code.co_firstlineno}:{code_digest}"
            )
            break
        # Retaining the code object prevents id reuse; using its identity rather
        # than CodeType equality avoids merging structurally equal code in two files.
        self._identities[id(code)] = (code, identity)
        return identity

    def _branch(self, code: CodeType, offset: int, destination: int) -> None:
        if self._active:
            try:
                identity = self._identity(code)
            except (OSError, ValueError) as error:
                self._error = error
                return
            if identity is not None:
                self._features.add(f"{identity}:{offset}:{destination}")

    def start_case(self) -> None:
        """Begin an empty case; fail rather than merge overlapping observations."""
        if self._closed or self._active:
            message = "monitoring case is closed or already active"
            raise RuntimeError(message)
        self._features.clear()
        self._error = None
        self._active = True

    def finish_case(self) -> tuple[str, ...]:
        """End the active case and return only its sorted branch identities."""
        if not self._active:
            message = "no active monitoring case"
            raise RuntimeError(message)
        self._active = False
        if self._error is not None:
            message = f"monitoring source identity failed: {self._error}"
            raise RuntimeError(message) from self._error
        return tuple(sorted(self._features))

    def close(self) -> None:
        """Release only this monitor's slot, once; never restart global events."""
        if not self._closed:
            self._active = False
            sys.monitoring.free_tool_id(self._slot)
            self._identities.clear()
            self._sources.clear()
            self._closed = True
