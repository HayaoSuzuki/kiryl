# Copyright (c) 2026 kiryl contributors
"""Synthetic comparison properties, discovered only by the benchmark harness."""

import os

from discovery import Store, route
from hypothesis import given, settings
from hypothesis import strategies as st
from hypothesis.database import DirectoryBasedExampleDatabase
from hypothesis.stateful import RuleBasedStateMachine, invariant, rule

COMPARISON = settings(
    max_examples=1_000_000,
    deadline=None,
    stateful_step_count=10,
    database=DirectoryBasedExampleDatabase(os.environ["KIRYL_BENCHMARK_DATABASE"]),
)


@COMPARISON
@given(st.binary(min_size=2, max_size=2))
def test_branching(data: bytes) -> None:
    """Every encoded request should be accepted by the synthetic router."""
    assert route(data)


class StoreMachine(RuleBasedStateMachine):
    """Compare the authored store with an independent reference value."""

    def __init__(self) -> None:
        """Keep application and reference state separate."""
        super().__init__()
        self.store = Store()
        self.expected: int | None = None

    @rule(value=st.integers(0, 15))
    def create(self, value: int) -> None:
        """Create in both models."""
        self.store.create(value)
        self.expected = value

    @rule(value=st.integers(0, 15))
    def update(self, value: int) -> None:
        """Update only an existing reference item."""
        self.store.update(value)
        if self.expected is not None:
            self.expected = value

    @rule()
    def delete(self) -> None:
        """Leave an empty reference state after every deletion."""
        self.store.delete()
        self.expected = None

    @invariant()
    def matches_reference(self) -> None:
        """Detect application defects by equality with the reference."""
        assert self.store.value == self.expected


TestStore = StoreMachine.TestCase
TestStore.settings = COMPARISON
