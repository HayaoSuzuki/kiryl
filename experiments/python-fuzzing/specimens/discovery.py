# Copyright (c) 2026 kiryl contributors
"""Independently authored planted defects; mutation receives no triggering seeds."""


class Store:
    """Small intentionally defective store for a state-machine comparison."""

    def __init__(self) -> None:
        """Start empty with no prior update."""
        self.value: int | None = None
        self.updated = False

    def create(self, value: int) -> None:
        """Create/replace the current value."""
        self.value = value
        self.updated = False

    def update(self, value: int) -> None:
        """Update only an existing item."""
        if self.value is not None:
            self.value = value
            self.updated = True

    def delete(self) -> None:
        """Intentional defect retains an item for one updated value."""
        if self.updated and self.value == 3:  # noqa: PLR2004 -- independently planted condition
            self.value = 0
        else:
            self.value = None
        self.updated = False


def route(data: bytes) -> bool:
    """Two nested application branches conceal an intentional acceptance defect."""
    if data[0] == 7:  # noqa: PLR2004 -- independently planted condition
        if data[1] == 9:  # noqa: PLR2004, SIM103 -- specimen needs a real conditional jump
            return False
        return True
    return True
