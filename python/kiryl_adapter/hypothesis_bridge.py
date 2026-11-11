# Copyright (c) 2026 kiryl contributors
"""Exact-version integration seam; no Hypothesis implementation types escape it.

The public fuzz handle cannot bind pytest's remaining arguments. The adapter
constructs the same execution context with explicit fixture/self bindings instead.
Source API reference: Hypothesis 6.170.13 core.py (MPL-2.0), inspected locally.
"""

import inspect
import json
import sys
import time
import traceback
from dataclasses import dataclass
from functools import wraps
from random import Random
from typing import TYPE_CHECKING, NotRequired, Protocol, TypedDict, cast

import hypothesis
import pytest
from hypothesis import settings, stateful
from hypothesis.core import (
    HypothesisHandle,
    StateForActualGivenExecution,
    failure_exceptions_to_catch,
    get_random_for_wrapped_test,
    process_arguments_to_given,
    skip_exceptions_to_reraise,
)
from hypothesis.database import choices_from_bytes, choices_to_bytes
from hypothesis.errors import FailedHealthCheck, InvalidArgument, UnsatisfiedAssumption
from hypothesis.internal.conjecture.choice import ChoiceTemplate, ValueHole
from hypothesis.internal.conjecture.data import ConjectureData, ConjectureResult, Status, StopTest
from hypothesis.internal.conjecture.engine import ConjectureRunner
from hypothesis.internal.conjecture.providers import BytestringProvider
from hypothesis.internal.escalation import InterestingOrigin
from hypothesis.internal.reflection import function_digest
from hypothesis.stateful import get_state_machine_test

if TYPE_CHECKING:
    from collections.abc import Callable

    from hypothesis.internal.conjecture.shrinker import Shrinker

from runtime import HYPOTHESIS_VERSION

SUPPORTED_VERSION = HYPOTHESIS_VERSION


class _Prefix(TypedDict, total=False):
    choice: str
    template: int | None


class _ReplayEnvelope(TypedDict):
    version: int
    hypothesis: str
    python: list[int]
    nodeid: str
    pytest_args: list[str]
    kind: str
    payload: str
    fingerprint: NotRequired[str]
    prefix: NotRequired[list[_Prefix]]
    random: NotRequired[list[object] | None]
    max_choices: NotRequired[int | None]


class _Wrapped(Protocol):
    hypothesis: HypothesisHandle
    _hypothesis_internal_use_settings: settings

    def __call__(self, *args: object, **kwargs: object) -> None: ...


@dataclass
class Evaluation:
    """One bounded execution's outcome, without Hypothesis-owned objects."""

    kind: str
    canonical: bytes | None = None
    fingerprint: str | None = None
    replay_token: bytes | None = None
    rendered: str | None = None
    database_replay: bool = False


@dataclass
class ShrinkEvidence:
    """Immutable original and best replay record under an explicit budget."""

    original: bytes
    best: bytes
    fingerprint: str
    complete: bool
    rendered: str | None = None
    verification: str = "pending"


def unpack_token(token: bytes) -> _ReplayEnvelope:
    """Reject incompatible replay envelopes before touching a target."""
    value = json.loads(token)
    if (
        not isinstance(value, dict)
        or type(value.get("version")) is not int
        or value.get("version") != 1
        or value.get("hypothesis") != SUPPORTED_VERSION
        or value.get("python") != list(sys.version_info[:2])
        or not all(isinstance(value.get(key), str) for key in ("nodeid", "kind", "payload"))
        or not isinstance(value.get("pytest_args"), list)
        or not all(isinstance(arg, str) for arg in value["pytest_args"])
    ):
        message = "incompatible replay token"
        raise ValueError(message)
    return cast("_ReplayEnvelope", value)


class BoundTarget:
    """One pytest invocation's bindings, never shared between parameter variants."""

    def __init__(self, item: pytest.Function) -> None:
        """Bind pytest's fixture/self arguments before creating execution state."""
        if hypothesis.__version__ != SUPPORTED_VERSION:
            message = f"unsupported Hypothesis version: {hypothesis.__version__}"
            raise RuntimeError(message)
        wrapped = item.obj
        if factory := getattr(wrapped, "_hypothesis_state_machine_class", None):
            wrapped = get_state_machine_test(factory)
        wrapped = cast("_Wrapped", wrapped)
        underlying = getattr(wrapped, "__func__", wrapped)
        inner = wrapped.hypothesis.inner_test
        if inspect.iscoroutinefunction(inner) or inspect.isasyncgenfunction(inner):
            message = "async properties are unsupported by the synchronous probe"
            raise RuntimeError(message)
        supplied = {name: item.funcargs[name] for name in inspect.signature(wrapped).parameters}
        receiver = getattr(wrapped, "__self__", None)
        if receiver is not None:
            supplied[next(iter(inspect.signature(underlying).parameters))] = receiver
        _, _, stuff = process_arguments_to_given(
            underlying,
            (),
            supplied,
            wrapped.hypothesis._given_kwargs,
            dict(inspect.signature(underlying).parameters),
        )
        self.settings = settings(wrapped._hypothesis_internal_use_settings, deadline=None)
        self.inner = inner
        self.item = item
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

    def token(self, kind: str, payload: str, **extra: object) -> bytes:
        """Wrap replay bytes with test, environment, and invocation identity."""
        return json.dumps(
            {
                "version": 1,
                "hypothesis": SUPPORTED_VERSION,
                "python": list(sys.version_info[:2]),
                "nodeid": self.item.nodeid,
                "pytest_args": list(self.item.config.invocation_params.args),
                "kind": kind,
                "payload": payload,
                **extra,
            }
        ).encode()

    def check(self, buffer: bytes) -> Evaluation:
        """Execute one buffer, retaining rejected input and failure distinctions."""
        data = ConjectureData(
            random=None, provider=BytestringProvider, provider_kw={"bytestring": buffer}
        )
        return self.evaluate(data, self.token("raw", buffer.hex()))

    def evaluate(
        self, data: ConjectureData, token: bytes, *, freeze: bool = True, persist: bool = True
    ) -> Evaluation:
        """Classify one case; Hypothesis choice storage remains bridge-owned."""
        try:
            self.state.execute_once(data)
            return Evaluation(
                "pass",
                canonical=(
                    bytes(data.provider.drawn)
                    if isinstance(data.provider, BytestringProvider)
                    else None
                ),
            )
        except skip_exceptions_to_reraise():
            return Evaluation("skip")
        except StopTest, UnsatisfiedAssumption:
            return Evaluation("reject")
        except InvalidArgument, FailedHealthCheck:
            raise
        except failure_exceptions_to_catch() as error:
            origin = InterestingOrigin.from_exception(error)
            frames = traceback.extract_tb(error.__traceback__)
            location = frames[-1] if frames else None
            if persist and self.settings.database is not None:
                self.settings.database.save(self.database_key, choices_to_bytes(data.choices))
            self.last_choices = data.choices
            self.last_origin = origin
            representation = "\n".join(data._stateful_repr_parts or ()) or self.state._string_repr
            envelope = unpack_token(token)
            envelope["fingerprint"] = str(origin)
            return Evaluation(
                "failure",
                fingerprint=str(origin),
                replay_token=json.dumps(envelope).encode(),
                rendered=f"{representation}\n{type(error).__name__} at {location}",
                database_replay=self.settings.database is not None,
            )
        finally:
            if freeze:
                data.freeze()

    def check_token(self, token: bytes) -> Evaluation:
        """Replay raw bytes or a deterministic choice-prefix candidate safely."""
        value = unpack_token(token)
        if value["nodeid"] != self.item.nodeid:
            message = "replay token target mismatch"
            raise ValueError(message)
        if value["kind"] == "raw":
            return self.check(bytes.fromhex(value["payload"]))
        if value["kind"] != "choices":
            message = "unknown replay token kind"
            raise ValueError(message)
        prefix = []
        for encoded in value.get("prefix", []):
            if "template" in encoded:
                prefix.append(ChoiceTemplate("simplest", encoded["template"]))
            else:
                choices = choices_from_bytes(bytes.fromhex(encoded["choice"]))
                if choices is None or len(choices) != 1:
                    message = "invalid replay token choice"
                    raise ValueError(message)
                prefix.extend(choices)
        random = Random(0)  # noqa: S311 -- deterministic replay, not cryptography
        state = value.get("random")
        if state is not None:
            random.setstate(
                (
                    cast("int", state[0]),
                    tuple(cast("list[int]", state[1])),
                    cast("float | None", state[2]),
                )
            )
        data = ConjectureData(random=random, prefix=prefix, max_choices=value.get("max_choices"))
        return self.evaluate(data, token)

    def _choice_token(self, data: ConjectureData) -> bytes:
        prefix = []
        for choice in data.prefix or ():
            if isinstance(choice, ChoiceTemplate):
                prefix.append({"template": choice.count})
            elif isinstance(choice, ValueHole):
                message = "opaque strategy inversion has no safe replay token"
                raise TypeError(message)
            else:
                prefix.append({"choice": choices_to_bytes((choice,)).hex()})
        return self.token(
            "choices",
            "",
            prefix=prefix,
            max_choices=data.max_choices,
            random=data._random.getstate() if data._random else None,
        )

    def minimize(
        self, original: bytes, seconds: float, before_case: Callable[[bytes], None]
    ) -> ShrinkEvidence:
        """Use Hypothesis's choice shrinker, acknowledging each candidate first."""
        if seconds <= 0:
            return ShrinkEvidence(
                original, original, unpack_token(original).get("fingerprint", ""), complete=False
            )
        deadline = time.monotonic() + seconds
        before_case(original)
        if time.monotonic() >= deadline:
            return ShrinkEvidence(
                original, original, unpack_token(original).get("fingerprint", ""), complete=False
            )
        observed = self.check_token(original)
        fingerprint = unpack_token(original).get("fingerprint", observed.fingerprint or "")
        result = ShrinkEvidence(
            original, original, fingerprint, complete=False, rendered=observed.rendered
        )
        if observed.kind != "failure" or observed.fingerprint != fingerprint:
            result.verification = "unstable"
            return result
        result.verification = "confirmed"
        representations: dict[bytes, str | None] = {}

        def execute(data: ConjectureData) -> None:
            if time.monotonic() >= deadline:
                raise _BudgetExpiredError
            token = self._choice_token(data)
            before_case(token)
            if time.monotonic() >= deadline:
                raise _BudgetExpiredError
            evaluation = self.evaluate(data, token, freeze=False, persist=False)
            if evaluation.kind == "failure" and evaluation.fingerprint == fingerprint:
                representations[choices_to_bytes(data.choices)] = evaluation.rendered
                data.mark_interesting(self.last_origin)

        runner = ConjectureRunner(
            execute,
            settings=settings(self.settings, database=None),
            random=Random(0),  # noqa: S311 -- deterministic shrinking
        )
        shrinker = None
        try:
            initial = runner.cached_test_function(self.last_choices)
            if isinstance(initial, ConjectureResult) and initial.status == Status.INTERESTING:
                shrinker = runner.new_shrinker(
                    initial,
                    lambda candidate: (
                        candidate.status == Status.INTERESTING
                        and isinstance(candidate, ConjectureResult)
                        and str(candidate.interesting_origin) == fingerprint
                    ),
                )
                shrinker.shrink()
                result.complete = True
        except _BudgetExpiredError:
            pass
        return self._finish_shrink(result, shrinker, representations)

    def _finish_shrink(
        self,
        result: ShrinkEvidence,
        shrinker: Shrinker | None,
        representations: dict[bytes, str | None],
    ) -> ShrinkEvidence:
        if shrinker is not None:
            best = shrinker.shrink_target
            value = unpack_token(self._choice_token(ConjectureData.for_choices(best.choices)))
            value["fingerprint"] = result.fingerprint
            result.best = json.dumps(value).encode()
            result.rendered = representations.get(choices_to_bytes(best.choices), result.rendered)
        return result


class _BudgetExpiredError(Exception):
    """Stop semantic minimization without replacing its best observed candidate."""


def bind_target(item: pytest.Function) -> BoundTarget:
    """Create an invocation-local binding behind the compatibility boundary."""
    return BoundTarget(item)


def _measure_handle(
    handle: HypothesisHandle,
    before: Callable[[], None],
    after: Callable[[Evaluation], None],
) -> Callable[[], None]:
    original = handle.inner_test

    @wraps(original)
    def measured(*args: object, **kwargs: object) -> object:
        before()
        outcome = Evaluation("pass")
        try:
            return original(*args, **kwargs)
        except skip_exceptions_to_reraise():
            outcome = Evaluation("skip")
            raise
        except StopTest, UnsatisfiedAssumption:
            outcome = Evaluation("reject")
            raise
        except failure_exceptions_to_catch() as error:
            outcome = Evaluation(
                "failure", fingerprint=str(InterestingOrigin.from_exception(error))
            )
            raise
        finally:
            after(outcome)

    if function_digest(measured) != function_digest(original):
        message = "measurement wrapper changes ordinary Hypothesis database identity"
        raise RuntimeError(message)
    handle.inner_test = measured

    def restore() -> None:
        handle.inner_test = original

    return restore


def instrument_ordinary(
    item: pytest.Function,
    before: Callable[[], None],
    after: Callable[[Evaluation], None],
) -> Callable[[], None]:
    """Observe native Hypothesis generation/shrinking with unchanged database identity."""
    if hypothesis.__version__ != SUPPORTED_VERSION:
        message = "unsupported Hypothesis measurement version"
        raise RuntimeError(message)
    if not getattr(item.obj, "_hypothesis_state_machine_class", None):
        return _measure_handle(cast("_Wrapped", item.obj).hypothesis, before, after)
    original_factory = stateful.get_state_machine_test

    def measured_factory(
        state_machine_factory: type[stateful.RuleBasedStateMachine],
        *,
        settings: hypothesis.settings | None = None,
        _min_steps: int = 0,
        _flaky_state: dict[str, bool] | None = None,
    ) -> Callable[..., None]:
        wrapped = original_factory(
            state_machine_factory,
            settings=settings,
            _min_steps=_min_steps,
            _flaky_state=_flaky_state,
        )
        _measure_handle(cast("_Wrapped", wrapped).hypothesis, before, after)
        return wrapped

    # Scoped runtime interception: unittest builds the @given wrapper lazily.
    # Explicit parameter types above preserve the pinned factory calling contract.
    patch = pytest.MonkeyPatch()
    patch.setattr(stateful, "get_state_machine_test", measured_factory)
    return patch.undo
