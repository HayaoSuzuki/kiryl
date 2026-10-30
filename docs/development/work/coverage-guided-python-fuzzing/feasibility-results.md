# Coverage-guided Python fuzzing: observed feasibility results

These observations concern experimental code, not the complete product.
Task 1 was executed on 2026-10-31 from documentation baseline `fed3bca` with
implementation sources subsequently committed as `7bf03ad`. Task 2 observations
below concern its subsequent working tree. Later stages are unrun.

## Task 1: pytest/Hypothesis binding and fixture lifecycles

Native macOS arm64 observations:

| Runtime | Integration suite | Scope |
| --- | --- | --- |
| CPython 3.14.9, Hypothesis 6.170.13, pytest 9.2.0 | 11 passed | Binding, finite invocations, fixture scopes, methods, selection, failures, generated schedules, Lean correspondence |
| CPython 3.15.2, same package pins | 11 passed | Same suite, native interpreter |
| Linux / Windows | Unrun | No support claim from these macOS runs |

The adapter collects once, uses distinct invocation nodes under shared pytest
parents, and invokes the ordinary runtest protocol. The exact-version bridge
binds fixture/self/parameter arguments into Hypothesis execution state directly.
It does not claim the public byte-only fuzz handle solved that integration.
Pytest's resolved fixture metadata is reused in this contained experimental seam.

Authored fixtures which deliberately share function state across examples
explicitly suppress Hypothesis's existing function-fixture health check, just as
ordinary Hypothesis tests must. The adapter does not silently suppress it.
Temporary specimens get their own pytest configuration so a mutation worker's
nested TMPDIR does not inherit a different project root and change their IDs.

### Five self-review passes

| Pass | Finding and action | Evidence |
| --- | --- | --- |
| 1: contract and scope | Missing resolved parameter-fixture metadata and original names broke fixture binding/IDs; preserve both when constructing invocation nodes | Parameter variants, method binding, function/module/session counter tests |
| 2: errors and boundaries | Collection errors were misreported as empty selections; preserve collection failure separately. Reject unknown node IDs and unsupported Hypothesis versions | Import-failure, empty-selection, unknown-ID, and exact-version tests |
| 3: state and side effects | Binding clones to the original bound method retained the class instance across slices; let pytest create a fresh receiver per invocation | Fresh-instance test observed RED then GREEN; scope counts distinguish per-input and per-invocation resets |
| 4: property/sensitivity | Exercise generated parameter schedules and formal count cases. A mutation baseline exposed inherited pytest root configuration; isolate the specimen project | 12 Hypothesis schedules, 9 Lean-generated cases; hoimin 0.14.45 killed one compare-equality mutant with fresh baseline exit 0 |
| 5: design and evidence | Keep compatibility state in the bridge, use a typed invocation-item subclass instead of assigning an untyped callback, and run strict Ruff/ty. No private target data used | Ruff ALL check, format check, ty all-error checks; two native interpreter suites |

### Formal claim and correspondence

Claim: each normally returning finite invocation contributes one function-fixture
setup and teardown; repeated normal invocations preserve balanced counts.

| Premise/observation | Lean representation | Experimental configuration/observation | Mode |
| --- | --- | --- | --- |
| N normally returning invocations | `ScopeModel.replay n` | `run_session` receives N invocations; fixture events and passing cases are returned | `strict` |
| All natural N | `replay_counts`, `normal_returns_balance` | Runtime comparison covers only generated N = 0 through 8 | `model-only` for the unbounded implementation claim |
| Missing/duplicate teardown and zero/one distinction | Fixed false-model witnesses | Deliberately broken model states, not modifications to pytest | `model-only` |

Lean 4.34.1 kernel-checked `replay_counts` and `normal_returns_balance` without
`sorry` or `native_decide`. The initial missing-finish implementation was rejected
by `decide`; correcting the transition made the proof compile. Fixed witnesses
detect transaction loss, duplicate teardown, and the zero/one boundary. The proof
is about this deliberately small model, not a proof of pytest or the adapter.

The non-imported `ScopeOracle` executable generated the checked-in TSV; Python
reads expectations from it instead of duplicating expected counts. The suite
observed matching counts for all nine same-premise cases. Bounds: N = 0..8,
two lifecycle events per normal invocation, nine configured cases; no exhaustive
arbitrary-event search. Lean builds have a 20-second external deadline and the
inductive proof a 10,000-heartbeat limit. The build reported 2.7 seconds for `ScopeModel` and 338 ms for `ScopeOracle`;
peak memory was not measured and bounds were not increased. Crashes, timeout,
fixture exceptions, interleaving, and arbitrary external state are outside this
model's theorem premises.

### Reproduction

From the repository root, after syncing the locked experiment environment:

```sh
uv sync --project experiments/python-fuzzing --frozen
experiments/python-fuzzing/.venv/bin/python -m pytest experiments/python-fuzzing/tests -q
experiments/python-fuzzing/.venv/bin/ruff check experiments/python-fuzzing
experiments/python-fuzzing/.venv/bin/ruff format --check experiments/python-fuzzing
experiments/python-fuzzing/.venv/bin/ty check --project experiments/python-fuzzing
```

Build Lean from `experiments/python-fuzzing/formal/` with the pinned toolchain
using `lake build` under a 20-second parent deadline, then run:

```sh
experiments/python-fuzzing/formal/.lake/build/bin/scope-oracle --check experiments/python-fuzzing/formal/scope-cases.tsv
```

For 3.15, sync the same lockfile into a separate environment using
`UV_PROJECT_ENVIRONMENT` and `uv sync --python 3.15`, then run its interpreter
with the same test arguments. The initial sandboxed package download failed DNS;
the approved retry fetched the locked package and the native run then passed.

The selected mutation changed `if case.kind == "failure"` to its opposite in
the adapter execution loop. It was planned and verified with hoimin 0.14.45,
`compare_eq_ne`, one worker, 8 GiB workspace limit, 10 GiB free-space reserve,
60-second total/10-second baseline/10-second mutant timeouts, and explicit
best-effort memory on macOS. Test argv was the absolute experiment interpreter
followed by `-m pytest -q`. One mutant was killed, none survived; this is a
targeted sensitivity check, not a mutation-score claim for the complete adapter.
All mutation manifests/workspaces were temporary and cleanup completed. The
earlier baseline failure was diagnosed via nested TMPDIR reproduction before
replanning with the corrected, isolated test project.

### Source snapshots

| Source | SHA-256 at observation |
| --- | --- |
| `adapter.py` | `d7936941fd9e6fd010900a70f9eed84e025fd256280a83025387bcd51583e7c4` |
| `hypothesis_bridge.py` | `f0d1dc940478c804c910bc5f13c269dcefd907abd11e2b8e4ccb2e04797e57ed` |
| `formal/ScopeModel.lean` | `6c03aaef4d438f4bc6efcbc92debf73a5f836e40d4564644fece05068b982c87` |
| `formal/ScopeOracle.lean` | `d3618443ebe6f4e905bc8565e5ef7be9b955d2b92d826ad757038001c7ee8270` |
| `formal/scope-cases.tsv` | `ab5489c2c248fdf450b78273fc9176f32be38bee89c307141ca5e5edc1b465bd` |

## Task 2: stateful replay, database identity, and semantic shrinking

Native macOS arm64: the combined suite passed 23 tests on CPython 3.14.9 and
3.15.2 with the same Hypothesis/pytest pins. Ordinary integer failures and
stateful invariant failures replayed in separate interpreters; semantic shrinking
produced smaller integer/operation-argument witnesses while retaining the
original and observed failure identity. These are integration probes, not a
complete native-crash-safe minimization system.

Database interoperability was checked with `Phase.reuse` only: ordinary pytest
had no generation phase to independently rediscover the planted bug. Both
ordinary and stateful failures replayed from saved Hypothesis choices. A saved
parameter variant did not contaminate another variant's database identity.
`database=None` still supported explicit replay and reported automatic database
replay unavailable. Raw fuzz inputs and Hypothesis choice encodings remain distinct.

| Self-review | Finding/action and evidence |
| --- | --- |
| 1: contract | Strengthened ordinary pytest checks to reuse-only for ordinary/stateful tests, and checked parameter-specific identities; an independently regenerated failure cannot pass this interoperability check |
| 2: boundaries | Added zero budget, incompatible token, and acknowledgment-delay tests; fixed executing a case after acknowledgment consumed its budget. Explicit version type checks reject bool/float coercion |
| 3: state | Fresh successful replay of a previous failure is unstable, not confirmed. Preserve that status during minimization. A unittest property exposed bypassed setup/teardown; run its receiver lifecycle around the finite invocation |
| 4: properties/sensitivity | 75 generated invalid schema versions plus existing generated schedules; targeted shrink-condition equality mutant killed after fresh baseline exit 0. Competing-failure test verifies the original assertion identity survives |
| 5: design/evidence | Typed compatibility envelope and contained Hypothesis stateful/shrink APIs; preserve original token and select the shrinker's best result rather than the last attempted failure. Ruff ALL, formatting, and ty all-error checks pass |

The shrink callback publishes a replay token before each execution and checks
remaining time both before and after acknowledgment. Choice-prefix templates
and PRNG state are encoded without pickle/eval; an opaque `ValueHole` that cannot
be encoded safely fails explicitly. Tested ordinary/stateful examples did not
need that unsupported inversion representation. This is a disclosed compatibility
limit to investigate if encountered, not evidence that every custom strategy works.

The standalone probe uses a fresh interpreter for replay and a pytest invocation
for semantic shrinking. The later Rust controller must supply process-tree limits,
bounded logs, fatal-crash recovery, and overall budget enforcement. The Python
helper's subprocess capture is not a production bounded-output guarantee.
No new Lean theorem is claimed for Hypothesis's shrinker; Task 1's normal-scope
correspondence remains exercised by the combined suite.

Commands: the Task 1 integration/Ruff/ty commands now include `tests/test_replay.py`.
The native 3.15 suite uses its separate locked environment. A targeted hoimin
0.14.45 plan mutated a `compare_eq_ne` condition in `BoundTarget.minimize`, with
the same one-worker/disk/time/best-effort limits as Task 1: one killed, zero
survived. Temporary mutation roots were cleaned.

Task 2 source snapshots (working tree after `7bf03ad`):

| Source | SHA-256 at observation |
| --- | --- |
| `adapter.py` | `71b5e60c83767d193c0dfb51a653835f04539a5a566d15abd1df395911d18b06` |
| `hypothesis_bridge.py` | `ab31411c0308f9c5dc4841e1b46aeefb2671e4101dd0e6abb49edb70f883b3da` |
| `tests/test_replay.py` | `045a2263c93d47e4a88ff4f5808a145c493a4b2328def1d2145ba1afd07fe43c` |

## Remaining gates

Per-case monitoring, Rust supervision, native crash recovery, OS process limits,
and coverage-guided discovery remain unverified. The sampled adapter/shrink
results above do not close those gates. No Rust experimental boundary exists
yet, so proptest/cargo-fuzz execution is pending Tasks 4-6, not reported as done.
Production engine choice and full platform acceptance remain open.
