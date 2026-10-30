# Copyright (c) 2026 kiryl contributors
"""Exact-version integration seam; no Hypothesis implementation types escape it.

The public fuzz handle cannot bind pytest's remaining arguments. This experiment
constructs the same execution context with explicit fixture/self bindings instead.
Source API reference: Hypothesis 6.170.13 core.py (MPL-2.0), inspected locally.
"""

import inspect
import traceback
from dataclasses import dataclass
from typing import TYPE_CHECKING, cast

import hypothesis
from hypothesis import settings
from hypothesis.core import (
    StateForActualGivenExecution,
    get_random_for_wrapped_test,
    process_arguments_to_given,
)
from hypothesis.errors import UnsatisfiedAssumption
from hypothesis.internal.conjecture.data import ConjectureData, StopTest
from hypothesis.internal.conjecture.providers import BytestringProvider
from hypothesis.internal.escalation import InterestingOrigin
from hypothesis.internal.reflection import function_digest

if TYPE_CHECKING:
    import pytest

SUPPORTED_VERSION = "6.170.13"


@dataclass
class Evaluation:
    """One bounded execution's outcome, without Hypothesis-owned objects."""

    kind: str
    canonical: bytes | None = None
    fingerprint: str | None = None
    replay_token: bytes | None = None
    rendered: str | None = None


class BoundTarget:
    """One pytest invocation's bindings, never shared between parameter variants."""

    def __init__(self, item: pytest.Function) -> None:
        """Bind pytest's fixture/self arguments before creating execution state."""
        if hypothesis.__version__ != SUPPORTED_VERSION:
            message = f"unsupported Hypothesis version: {hypothesis.__version__}"
            raise RuntimeError(message)
        wrapped = item.obj
        underlying = getattr(wrapped, "__func__", wrapped)
        inner = wrapped.hypothesis.inner_test
        supplied = {name: item.funcargs[name] for name in inspect.signature(wrapped).parameters}
        if getattr(wrapped, "__self__", None) is not None:
            supplied[next(iter(inspect.signature(underlying).parameters))] = wrapped.__self__
        _, _, stuff = process_arguments_to_given(
            underlying,
            (),
            supplied,
            wrapped.hypothesis._given_kwargs,
            dict(inspect.signature(underlying).parameters),
        )
        self.settings = settings(wrapped._hypothesis_internal_use_settings, deadline=None)
        self.inner = inner
        self.wrapped = underlying
        self.database_key = function_digest(inner) + b".secondary"
        self.state = StateForActualGivenExecution(
            stuff,
            inner,
            self.settings,
            get_random_for_wrapped_test(inner, underlying),
            underlying,
            thread_overlap={},
        )

    def check(self, buffer: bytes) -> Evaluation:
        """Execute one buffer, retaining rejected input and failure distinctions."""
        data = ConjectureData(
            random=None, provider=BytestringProvider, provider_kw={"bytestring": buffer}
        )
        try:
            self.state.execute_once(data)
            return Evaluation(
                "pass", canonical=bytes(cast("BytestringProvider", data.provider).drawn)
            )
        except StopTest, UnsatisfiedAssumption:
            return Evaluation("reject")
        except Exception as error:  # noqa: BLE001 -- the test oracle reports unhandled exceptions
            origin = InterestingOrigin.from_exception(error)
            frames = traceback.extract_tb(error.__traceback__)
            location = frames[-1] if frames else None
            return Evaluation(
                "failure",
                fingerprint=str(origin),
                replay_token=buffer,
                rendered=f"{type(error).__name__} at {location}",
            )
        finally:
            data.freeze()


def bind_target(item: pytest.Function) -> BoundTarget:
    """Create an invocation-local binding behind the compatibility boundary."""
    return BoundTarget(item)
