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

## Task 3: isolated application branch feedback

The combined suite passed 36 tests on native macOS arm64 CPython 3.14.9 and
3.15.2. With pytest-cov 7.1.0 / coverage 7.17.0 and branch collection enabled,
both runtimes also passed all 36. A separate 3.14 run using
`COVERAGE_CORE=ctrace` with `--cov-context=test` passed all 36.

Known upstream limits are retained: coverage's `sysmon` core warned that dynamic
contexts are incomplete; that configuration is not claimed supported for accurate
context attribution. coverage.py also raised before our callback for an artificially
NUL-containing code filename. The adapter's NUL defense is tested in a child
without upstream coverage hooks, not presented as evidence for that unsupported
combined configuration. The initial full coexistence run failed that one boundary
case; separating the premises restored a meaningful combined suite.

| Self-review | Finding/action and evidence |
| --- | --- |
| 1: contract | Repeated left/right cases have isolated edges; collected oracle/test files and framework paths are excluded, and actual coverage roots are returned. IDs match a fresh interpreter |
| 2: errors/boundaries | Exhausted slots refuse execution without stealing another tool. Source read/invalid filename errors are deferred as monitoring infrastructure failures instead of injected target exceptions. Generated `<string>` code is excluded rather than read as a file |
| 3: state/ownership | CodeType structural equality merged different source files; cache by object identity while retaining the object to prevent ID reuse. Closing twice cannot free a slot reused by another client. Fresh replay's module alias preserves fixture stash identity |
| 4: properties/sensitivity | Up to 20 generated true/false schedules, repeated-edge reset checks, independent client disabled-event and new-branch delivery checks. A method-call-removal mutant deleting the feature reset was detected after a fresh green baseline |
| 5: design/evidence | Stable source/code/environment IDs stay inside the Python owner; no global restart_events calls. Ruff ALL/format and ty all-error pass. Coverage coexistence limitations and bounded timing observations are explicit |

The reproducible `measure_monitor.py` runs the same 2,000 fixed-input adapter cases
with collection off/on. One local observation recorded 159,693,417 ns without
monitoring and 194,432,625 ns with monitoring, including pytest setup. This is a
same-process pair with disabled mode first, not a statistical performance claim,
an IPC measurement, or a bug-discovery comparison. No acceptance threshold was
invented. The off switch belongs to this experiment/ablation interface, not a
product promise of unguided operation.

Coexistence commands extend the integration-suite command with
`--cov=monitor --cov-branch --cov-report=term`; the contexts probe adds
`--cov-context=test` and explicitly selects `COVERAGE_CORE=ctrace` for its supported
observation. Set `COVERAGE_FILE` to a temporary result path so generated coverage
data is not repository evidence. The behavioral checks, not the reported
statement percentage, establish the observed coexistence slice.

Task 3 source snapshots (working tree after `a367509`):

| Source | SHA-256 at observation |
| --- | --- |
| `monitor.py` | `fc6c5cbf33ada4a7c0aaf77154d431f0aa76eba15009505a82c4dca3e450fac3` |
| `adapter.py` | `a1668f770bd286098017e6f3a819d775f1e7f4d05b4c54089bb886bf1eeff214` |
| `tests/test_monitor.py` | `6b9efe9775d7216479feb4f215e17d9bd92a0f27ab42f11f005e9236393a0211` |
| `measure_monitor.py` | `6d47a2efd2820b4ea8734deeea22fbebce0b7f6baadad861c93493f83392f012` |

## Task 4: Rust supervision and input ownership

Native macOS arm64 CPython 3.14.9 and 3.15.2 each passed 48 experiment tests.
The standalone Rust suite passed eight tests, including proptest. Ruff ALL,
formatting, ty all-error, Rust formatting/Clippy with warnings denied, and
cargo-deny 0.20.2 license/source checks passed. Dependencies belong only to the
unpublished experimental workspace; the product crate remains the scaffold.

The real parent retained exact raw bytes through `os.abort`, verified a native
crash in another interpreter, and continued an independent test. Assertions and
`pytest.fail` are verified using opaque bridge tokens in fresh workers. Semantic
minimization acknowledges each candidate before execution; an abort during that
phase retains both the immutable raw original and the active opaque token in a
separate lineage record. Returned best tokens remain pending fresh verification;
the probe does not assert they were verified merely because shrinking returned.

FD-level stdout joins stderr as binary logs while a reserved, noninheritable
stream carries bounded length-prefixed JSON. A binary flood exceeding 500 KiB
retained at most 1,024 bytes in the configured tail without breaking control.
All logs are drained concurrently. A malformed control frame is infrastructure.
Closing the control descriptor while the process remains alive initially blocked
the supervisor and mislabeled the subsequent exit; a failing regression led to
bounded `try_wait` supervision. Normal signal/exception exits are crash candidates;
arbitrary positive exit statuses and transport failures are infrastructure.

Case timeout, whole-campaign cancellation, and orderly interruption are distinct.
Campaign expiry wins simultaneous case expiry; Ctrl-C returns 130 with a report.
The total budget includes setup/replay/minimization. Slices start after setup and
finish active examples cooperatively; a short slice initially starved before its
first input and was corrected. Two finite slow cases finish and run teardown in
separate worker invocations. Forced termination does not promise Python teardown.
An exhausted verification budget leaves the stored original pending.

| Self-review | Finding/action and evidence |
| --- | --- |
| 1: contract | Real crash, independent continuation, FD flood, fixture error, assertion/native fresh replay; pytest.fail initially misclassified because it inherits BaseException, then regression RED to GREEN using the pinned Hypothesis failure policy |
| 2: boundaries | Fixed setup-starved slices and blocking wait after control EOF; tested case/global/interrupt distinctions and pending verification |
| 3: state/storage | Original precedes acknowledgment; shrink crash retains token; separate lineage writes; incompatible exact versions, failed writes, and reopened stores preserve original bytes |
| 4: formal/properties | Lean budget precedence and abstract original preservation; eight generated budget cases against the live Rust expiry function; proptest framing, suffixes, and real artifact operations |
| 5: design/evidence | Exact-version Python seam remains isolated; strict Python/Rust checks, both native interpreters, bounded fuzzing and license/source checks; no product/platform adoption claim |

Lean 4.34.1 built `BudgetModel` and `budget-oracle` under an external 20-second
bound. The abstract finding theorem proves append operations preserve the original
for any list of derived candidates. It does not prove filesystem atomicity or the
whole controller. The budget correspondence exercises the actual Rust helper under
injected Boolean clock predicates (`internal-fixture`), separately from real-time
process tests. Corpus freshness is checked by the generated executable.

With nightly-2026-10-08 and cargo-fuzz 0.13.2, `wire` and `tail` each completed
10,000 libFuzzer executions without a crash. Each invocation used
`-max_total_time=10 -runs=10000 -timeout=1 -rss_limit_mb=512`. The wire target feeds
both payload decoding and complete framing; the tail target checks exact retained
suffixes across chunk boundaries. These bounded observations are not proofs of
absence of bugs. The fuzz workspace has its own lockfile and ignores generated
corpora/artifacts; meaningful discovered regressions belong in ordinary tests.

Reproduction uses the commands in the plan with the experiment interpreter
instead of an implicit environment. Run cargo-deny with global options before
`check`: `cargo deny --manifest-path experiments/python-fuzzing/Cargo.toml
--config experiments/python-fuzzing/deny.toml --locked check --deny warnings
licenses sources`. The CLI refuses unavailable hard memory enforcement by default;
Task 4's process probes explicitly opted into best effort. Current process-group
cleanup is weaker than containment: deliberately detached descendants are outside
that guarantee. Task 5 owns capability enforcement and native OS evidence.

The probe starts a fresh process for each finite invocation/verification/shrink.
Within-worker pytest scope behavior is evidenced by Task 1. These timings include
additional setup and do not measure a production scheduler. POSIX control-FD
corruption/closure are deliberate trusted-bootstrap fault injections, not security
sandbox tests. Native Windows/Linux results and full campaign scheduling remain
unverified. A finite fixed schedule bounds this stage's retained report size;
streaming search must additionally bound report accumulation in Task 6.

Task 4 source snapshots (working tree after `624775b`):

| Source | SHA-256 at observation |
| --- | --- |
| `Cargo.toml` | `6c97cd4397d40b150d79264e09e61296f32ec4a2ca4849ccf56e472fb0d7e012` |
| `src/controller.rs` | `b29c9ff68fea2dc60fbec6fef393adbde54fa21b41380d7495513d394eb120ca` |
| `src/artifacts.rs` | `e23b78421c27b971b88808fffa6c0c310bba61e468af2021c50b13ed6047a8b4` |
| `src/wire.rs` | `6368f284074fc9444b1377bca81e23c767199d310fdf8381ec37f9135b435592` |
| `src/main.rs` | `c2d8d366b8f96e3af8f1e53e7129a918885b0c4b1ad4b66cd7e74ac135e28d13` |
| `worker.py` | `34674cff0a1661b3dd061afede1d3a0ed5ca7b6c397aad65933c5b8de9c14015` |
| `wire.py` | `012c1253559b4dea3311949c2487028fb9af95c5a18e97b7778f9fb184631925` |
| `adapter.py` | `3ad12676b21a712222ac6b7f36d7df78c73ad32e4c4f49fba959fbba3b1318ca` |
| `hypothesis_bridge.py` | `8f98e9b2472560762f90cf51f04568193519de9142d1802f6a081ff9563182b0` |
| `tests/test_controller.py` | `ef858f9ce65866bbf745dd5572a8466771626c2a308734f527279c82a9c09881` |
| `formal/BudgetModel.lean` | `34f0833a5a90ba9ea4bc6f6d35ecfd066b5674d438da42460eaafb8c022d822d` |
| `formal/budget-cases.tsv` | `5d7a08608b54635f2dcd423a57423c358224827c10ec1eec152a71fcc7a7aba8` |

## Task 5: Resource capabilities and startup ordering

Native macOS arm64 CPython 3.14.9/3.15.2 each passed 55 tests with two explicit
hard-backend skips. Rust passed ten tests. The experiment does not have a macOS
hard memory backend: default runs refuse before collection; explicit best effort
reports `tree_memory_hard=false`, `tree_cleanup=false`, and a concrete reason.
Ordinary inheriting descendants are killed with their process group. Detached
descendants are outside that guarantee. Task 4's cleanup flag described ordinary
group cleanup and was too strong as a containment claim; this stage corrects it.

The controller originally allowed PYTHONPATH startup hooks before attachment.
A failing synthetic sitecustomize regression led to an isolated `-I -S` bootstrap.
Only after resource assignment and the start acknowledgment does it initialize
site packages and import pytest/targets. This experiment intentionally ignores
PYTHONPATH and user-site startup configuration; install dependencies into the
chosen interpreter or use pytest's own path configuration. Product import policy
remains a follow-on decision. This is process ownership, not a security sandbox.

Linux cgroup v2 support resolves current unified membership/mounts, creates an
owned group, opens existing kernel control files, verifies the effective memory
limit, enables group OOM handling, probes empty-group kill permission, and assigns
the worker before acknowledgment. A real sleeping-child probe checks assignment
and cleanup permissions. Without writable delegation it fails closed or explicitly
falls back. The backend never changes an existing group's limits or enables
controllers in a shared parent. `memory.events` oom_kill evidence, not an arbitrary
signal, establishes an observed OOM. Unsupported namespace mapping fails closed.

Windows Job Objects configure aggregate job memory, kill-on-close and a completion
port before assignment. The capability probe assigns a trusted OS child and
checks cleanup. Memory-limit notifications supply backend provenance. The worker
suppresses CRT dialogs/report-fault before site initialization. Microsoft's
[abort contract](https://learn.microsoft.com/en-us/cpp/c-runtime-library/reference/abort)
explains that without reporting, abort can terminate with code 3. Such a status is
an ambiguous native-crash candidate: an explicit exit(3) is indistinguishable.
Fresh reproduction confirms the same abnormal outcome, not its native cause.
Other positive exit codes remain infrastructure. Native Windows observations
are still required to accept this mechanism.

The mechanisms were informed by hoimin's fixed
[resource sources](https://github.com/HayaoSuzuki/hoimin/tree/542ec59c002d72fdff0137aa90113ffa59d70844/crates/hoimin-cli/src/resource)
under Elastic-2.0, read through gh. This synchronous probe does not import its
run-wide resource framework. Rust checks for x86_64-unknown-linux-gnu and
x86_64-pc-windows-gnu passed with Clippy all/pedantic and warnings denied. Cross
compilation establishes type compatibility only; it does not prove enforcement.

| Self-review | Finding/action and evidence |
| --- | --- |
| 1: contract/startup | Real hook initially executed before attachment; isolated bootstrap regression RED to GREEN; unavailable hard-cap default refusal precedes target marker |
| 2: boundaries/capabilities | Corrected portable containment flag; ordinary descendant cleanup, timeout flood, unrelated exit provenance; native-only aggregate/setup memory tests explicitly skipped locally |
| 3: state/ownership | Cleanup authority consumed before failure; actual Guard correspondence and detected retry-on-error mutation. Correlation validates before releasing raw input. Mixed finding/cancellation regression RED to GREEN |
| 4: native APIs | Reviewed fixed upstream cgroup/Job mechanisms, write/assignment order, kernel notification provenance and cleanup failures; successful cross checks kept distinct from native evidence |
| 5: design/evidence | One guard owns descendants; actual capability fields replace OS-name assumptions; strict Python/Rust checks, both local interpreters, documentation and source/license validation; unverified gates remain explicit |

The cleanup model proves at most one OS attempt for arbitrarily many requests.
Nine generated cases compare the actual Rust guard's failure consumption with
an injected invalid PID that fails before sending any signal (`internal-fixture`).
An intentionally moved consumption bit (after a fallible call) was detected;
restoring the correct order returns green. This theorem and its fixtures do not
prove cgroup/Job cleanup, PID scheduling, or operating-system calls. Real process
tests separately check ordinary descendants and bounded drain after termination.

A target finding followed by whole-budget expiry during teardown retains both
outcome categories and its pending original. Result correlation now validates
IDs/node IDs before relinquishing the active buffer; a malformed-ID fault retains
exact bytes in an infrastructure record. Limit failures during setup carry no
fabricated target input. Replay stopped by budget/infrastructure stays pending.

Task 5 source snapshots (working tree after `962198c`):

| Source | SHA-256 at observation |
| --- | --- |
| `src/resource/mod.rs` | `5b295ed80c864d19951e051ebf8b88305a8b5dd25bdbd73b3925bffe7889734e` |
| `src/resource/linux.rs` | `e6ad3966b3165e37696e4a4e42f2df79af750a0ac5fe9b3be204635c631eb7ac` |
| `src/resource/windows.rs` | `87366e796d911476d834fcfc957759ee00f01240dd6b1d6198a0139979ac1848` |
| `src/resource/portable.rs` | `451058e868d5619f17503c6f5d6bb17cb44015df7d91e78e6d66ff643bed673e` |
| `src/controller.rs` | `2029a3beb478f175056216a499857834d1efe5fa3f967a02793758ccba2551f4` |
| `worker.py` | `7511e4534a0bbf8a540798240cfa175100bdf0831e24a1410494c1c8617de5c6` |
| `tests/test_resource_probes.py` | `793c7eaa09fdb61e559eeaee96cc1aedfa247eccdd521517c604367ec9a4bbed` |
| `tests/test_controller.py` | `1fbdf097bbfa91a4d8f48a48c5bab0b0a350a66e0f6846f2043f3eceb12c1e4a` |
| `formal/ResourceModel.lean` | `2ee9a138fcb63ba4473845ac334812ca15963aa8c6e20f0a0ea273673c41974f` |
| `formal/resource-cases.tsv` | `a6abf5c676c978615f6e50ad126a8e9353f660214ff44d888b146416c37bff4c` |

## Task 6: Guided discovery and measured evidence

The completed local sample contains 60 comparisons: two native interpreters,
two authored targets, three modes, and seeds 0 through 4. Each comparison had a
20-second total limit including worker/pytest setup, replay and shrinking. Modes
used disjoint freshly created database/corpus parent directories; no known failure
input or previous database was an initial discovery seed. Order rotates by seed.
Both interpreter campaigns ran concurrently on the same host, with one Python
worker per independent campaign. Timing is illustrative, not an isolated CPU or
statistical performance experiment.

The compact observations retain exact source/binary hashes, versions, seeds,
limits, attempts, useful/rejected calls, branches, corpus sizes, discoveries,
first-discovery time, replay/shrink cost, adapter/roundtrip timing and actual
capabilities: [CPython 3.14](evidence/macos-python314.json) and
[CPython 3.15](evidence/macos-python315.json). Their `partial` status retains the
unverified hard-backend category; it does not discard the passing adapter and
fallback observations. Local raw records remain in the ignored experiment results
directories. The workflow retains its own raw bundles for seven days. Compact
summaries are observations, not standalone replay bundles.

| Native runtime | Target | Guided discoveries/fresh confirmations | Feedback disabled discoveries | Ordinary Hypothesis discoveries |
| --- | --- | --- | --- | --- |
| macOS CPython 3.14.9 | Nested branching | 5/5; 5/5 | 0/5 | 5/5 |
| macOS CPython 3.14.9 | Stateful store | 5/5; 5/5 | 5/5 | 5/5 |
| macOS CPython 3.15.2 | Nested branching | 5/5; 5/5 | 0/5 | 5/5 |
| macOS CPython 3.15.2 | Stateful store | 5/5; 5/5 | 5/5 | 5/5 |

Guided exploration retained four corpus entries for the branching specimen and
four to six for the stateful specimen. The feedback-disabled corpus stayed at its
single zero seed. Both modes still observed application branches; only retention
used feedback. Controlled same-input ablation and deterministic feedback-stream
proptest verify that this distinction is implemented. The sample establishes
branch-driven retention and replayable planted discoveries. It does not establish
superiority on real applications, against Hypothesis, or against any commercial
product. The stateful specimen was found by every mode, including mutation without
feedback.

The ordinary comparison uses pytest's normal Hypothesis generation, verification
and shrinking with measurement callbacks. Its stateful unittest wrapper constructs
`@given` lazily, so the pinned seam intercepts that factory for one synchronous
call and restores it. Runtime digest equality and reuse-only ordinary/stateful
database tests verify identity preservation. These callbacks add overhead and
stack frames. Ordinary counts include inner oracle replays/shrink calls but exclude
rejections occurring before the oracle is entered; kiryl search counts exclude its
subsequent verification/shrink. They are not comparable generation-rate estimates.
The authored properties explicitly disable Hypothesis deadlines in every mode.

`confirmed_original` is a kiryl fresh-replay measure and is not applied to ordinary
Hypothesis observations. Roundtrip minus adapter time includes serialization,
pipe exchange and scheduling; it is not pure IPC latency. The adapter timer covers
bridge draws/monitoring for mutation, and inner oracle/monitoring for ordinary
Hypothesis. Replay/shrink wall times include fresh-worker setup. Budget cancellation
is an explicit probe outcome and may produce exit1 without discovery; discovery
counts use the explicit observation field, not the exit code.

| Self-review | Finding/action and evidence |
| --- | --- |
| 1: contract/scope | Streaming feeds back before the next proposal; retained inputs canonicalize only after valid passes; native ordinary/stateful measurement preserves reuse-only DB identity. Async and Hypothesis configuration/health-check errors initially misclassified, then regressions RED to GREEN |
| 2: boundaries/errors | A requested large input could block the parent writer and hide its deadline; bounded writer thread regression RED to GREEN. Finding followed by fatal teardown could hide infrastructure; regression RED to GREEN. Unlimited campaigns had an unintended one-second shrink budget; slow-ack regression RED to GREEN |
| 3: state/side effects | WNOWAIT preserves the Unix identity anchor until cleanup; native waitpid regression RED to GREEN. macOS rejects zombie-only group signaling even outside sandbox; a live owned group member permits cleanup. Aggregate log/case histories stay bounded; comparison namespaces never share DB/corpus parents |
| 4: proof/properties/sensitivity | Lean retention/reject/count model and 21 generated boundary cases; deterministic-stream, framing, suffix and immutable-artifact proptest. Three cargo-fuzz targets each completed 10,000 runs. One Hypothesis measurement guard mutant was killed after a fresh passing baseline |
| 5: design/evidence | Rust owns proposals/time/processes/persistence; Python owns pytest/Hypothesis/monitoring. Strict checks and both native suites pass; all ten raw evidence files validate. The six-cell workflow is read-only, uses rechecked pinned actions and gh for Lean retrieval; no native result is inferred from configuration |

The final local suites passed 68 tests with three explicit hard-backend skips on
each interpreter. Rust passed 14 tests, including three search integration/property
checks. Ruff ALL/format and ty all-error checks pass; separate darwin/linux/win32
type checks cover conditional APIs. Both Linux/Windows cross-target Clippy checks
pass. The workflow passed actionlint/ShellCheck and offline zizmor, with no findings.
No root product behavior changed, so the user guide remains scaffold guidance.

Lean 4.34.1 builds four small models and executable oracle generators. The new
search theorem bounds abstract corpus counts for arbitrary observation sequences
and leaves rejects unchanged. Its count abstraction assumes fresh, bounded
canonical payloads; the real Rust correspondence uses explicit unique four-byte
fixtures across rejection, duplicate and saturation boundaries. This is an
`internal-fixture` count correspondence, not proof of byte mutation, strategy
semantics, the Python implementation, the operating system or discovery efficacy.
No `sorry`/`native_decide` is used; builds have external 20-second bounds.

With cargo-fuzz 0.13.2 and nightly-2026-10-08, `wire`, `tail` and `search` each
completed 10,000 executions with 10-second campaign bounds, 1-second input timeout
and 512 MiB RSS ceilings. The new target checks bounded proposals/corpora and reject
non-retention. Reproduction is in the experiment fuzz manifests; generated corpora
are ignored. These runs and the single killed mutation are finite sensitivity
observations, not complete coverage or proofs that no bugs remain.

Standalone and fuzz-workspace cargo-deny 0.20.2 license/source checks passed.
The [fuzz-only policy](../../../../experiments/python-fuzzing/fuzz/deny.toml)
adds an exact libfuzzer-sys 0.4.13 exception for its additional NCSA condition,
reviewed against the [SPDX license text](https://spdx.org/licenses/NCSA.html).
The root product allow-list is unchanged; dependency updates require re-review.

The [native workflow](../../../../.github/workflows/fuzzing-feasibility.yml) runs
behavioral probes and the fixed comparisons for all six OS/minor cells. Subsequent
maintainer PR authorization permits relevant pull-request runs in addition to
manual dispatch. Source installation and downloads are development setup; the
probe itself has no service account or outbound-network requirement. Full product
campaign scheduling, installation, standalone CLI replay/minimize and final engine
adoption remain dependent follow-on work, as scoped in the plan.

## Decision and remaining gates

| OS/runtime cell | Observed here | Resource boundary |
| --- | --- | --- |
| macOS / CPython 3.14 | 68 passed, 3 explicit skips; five-seed comparisons validated | Default refusal and explicit portable fallback observed; hard enforcement unavailable |
| macOS / CPython 3.15 | 68 passed, 3 explicit skips; five-seed comparisons validated | Same disclosed fallback boundary |
| Linux / CPython 3.14 | Native unrun; cross-target type/Clippy only | Requires actual delegated-cgroup or explicit fallback observations |
| Linux / CPython 3.15 | Native unrun; cross-target type/Clippy only | Same pending native evidence |
| Windows / CPython 3.14 | Native unrun; cross-target type/Clippy only | Requires actual Job assignment, aggregate memory and cleanup observations |
| Windows / CPython 3.15 | Native unrun; cross-target type/Clippy only | Same pending native evidence |

Architecture A is feasible for the locally observed test forms and branch-driven
search. Continue native experiments before production adoption. The next concrete
gates are real Linux/Windows behavioral runs, hard-memory/setup/concurrent-allowance
probes in capable environments, and campaign/CLI design against those observations.
An unavailable delegated cgroup is not a passing hard-enforcement result. These
limitations do not remove required stateful, fixture or platform scope.
