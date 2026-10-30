# Coverage-guided Python fuzzing: design proposal

Status: approved design direction, pending feasibility experiments. Product scope
was approved in the specification interview on 2026-10-31 (Q38); the maintainer
then selected architecture A: a Rust search controller driving separate Python
test workers (Q39). The maintainer also accepted the fixture, internal-API, and
durability choices below (Q40-Q42). After the complete document was presented,
the maintainer instructed the agent to proceed to the next step on 2026-10-31.
This authorizes implementation planning; it does not establish runtime results.
This document records that conversation without application-specific
examples. It does not establish implemented behavior or successful experiments.

Repository baseline inspected: `5c4eccb43708bc115c83bc2f468c22758d73b5e1`.
The package remains an empty Rust library scaffold. No fuzzing engine or Python
runtime dependency has been adopted.

## Intent and approved product scope

Provide a locally operated, independently maintainable tool for discovering
latent bugs in trusted Python code, with terms suitable for workplace use.
Use existing Hypothesis input definitions and test assertions. Improving on
HypoFuzz's discovery rate is not an initial acceptance requirement.

The following requirements were approved in the interview. They describe the
intended first version, not the current scaffold.

| Area | Approved requirement |
| --- | --- |
| Test interface | Reuse `@given`, strategies, assertions, and explicit preconditions. No new input-definition language. |
| Discovery | Coverage-guided exploration of Python branches. Native-extension coverage instrumentation is deferred; fatal native crashes are still findings. |
| Tests | Synchronous tests collected through pytest, including fixtures, parametrization, and Hypothesis stateful tests. Async execution and automatic conversion of fixed-input tests are excluded. |
| State | Follow ordinary pytest/Hypothesis fixture lifetimes. Tests explicitly reset databases, files, and other external state when needed. |
| Selection | Support pytest paths, node IDs, `-k`, and markers. |
| Platforms | macOS, Linux, Windows; CPython 3.14 and 3.15 with the GIL enabled. These are first-version acceptance targets, not demonstrated compatibility. |
| Parallelism | One worker by default. Multiple workers run different tests; never fuzz one test concurrently in several workers in the first version. |
| Scheduling | Equal exploration-time allocation across selected tests; adaptive allocation is deferred. |
| Time | With no campaign limit, run until interrupted. An explicit campaign budget includes search, verification, and minimization. Process cleanup and final persistence can take additional time. Also provide a per-case timeout. |
| Failure policy | Assertions and unhandled exceptions are failures. Expected exceptions handled by the test and rejected inputs are not. Distinguish target findings from collection, setup, protocol, and worker-launch errors. |
| After discovery | Preserve the original input before verification or minimization. Stop searching that test after its first finding; continue other tests. Retain unstable findings and unfinished verification/minimization with explicit status. |
| Limits | Memory limit applies separately to each worker process tree. If hard enforcement is unavailable, disclose that limitation and require explicit acceptance before running. Increasing workers increases aggregate allowance. |
| Output | Bound retained stdout/stderr, retain the tail, continue draining, and report truncation. Exceeding retained-output capacity alone is not a target failure. |
| Persistence | Keep useful inputs and findings locally for subsequent runs. Offer standalone replay and minimization. Reproduction assumes matching code, dependencies, OS, and other relevant execution conditions. |
| Compatibility | No backward compatibility, migrations, or old-runtime preservation for stored exploration data. Durable regression tests are the user's responsibility. |
| Regression handoff | Present available input values or operation sequences, the finding, and reproduction instructions. Generating standalone regression code for arbitrary types and tests is not required. |
| Settings | Input semantics remain in the test; kiryl owns campaign/example execution budgets. Explain which Hypothesis settings apply in fuzz mode. |
| Interface | CLI and machine-readable JSON reports. No GUI or dashboard, including in later scope. |
| Exit status | Findings, unresolved failure candidates, unstable findings, and execution-infrastructure errors produce nonzero exit status. Zero means no finding in the executed search, not proof of correctness. |
| Connectivity | kiryl needs no account or outbound network connection. The selected test's own communication is a separate concern. |
| Acceptance | Use distributable synthetic tests to establish exploration, planted-bug discovery, replay, minimization, limits, and cancellation across the platform matrix. Compare ordinary Hypothesis execution without requiring superior performance. |

### Non-goals and anti-goals

Do not infer business correctness without assertions or another test oracle.
Do not execute untrusted code as a security service or describe process limits
as a security sandbox. Do not add remote services, telemetry, automatic test
generation, source mutation, solvers, or a GUI to establish novelty.

Do not silently weaken resource guarantees, classify an arbitrary process
termination as OOM, replace a failure input with a different finding during
minimization, or report an unverified platform as supported. Deployment,
publishing, and release automation remain deferred.

## Approaches considered

| Approach | Advantages | Costs | Selection |
| --- | --- | --- | --- |
| A: Rust generates candidates and sends them to a Python worker | Parent retains the exact active input after worker death; Python integration uses a separate interpreter; no Rust Python-extension build is required | Per-case IPC and coverage-transfer overhead | Selected by the maintainer as the design and experiment direction |
| B: Rust supervisor with a Rust search extension inside each Python worker | Search and feedback stay in one process | Extension builds for the runtime matrix; worker death also loses in-process engine state; additional recovery coordination | Reconsider only if measured IPC costs justify it |

Atheris is not an established dependency. Its currently documented platform
and Python range does not establish the entire required matrix. A byte-mutating
Rust engine plus a Python coverage adapter is a candidate to experiment with,
not an adopted or measured engine. No generic multi-engine plugin interface
is required for the first implementation.

## Ownership and narrow interfaces

Use a small number of modules grouped by knowledge, rather than separate
modules for each chronological phase. A module here need not be a separate
crate or public API.

| Owner | One-sentence responsibility | Knowledge hidden from callers |
| --- | --- | --- |
| Campaign (Rust) | Allocates budget and drives selected tests to completed, finding, interrupted, or infrastructure-error outcomes. | Fair scheduling, finding workflow, remaining-budget accounting, report aggregation |
| Search (Rust) | Proposes opaque candidate bytes and retains inputs that improve exploration. | Mutation rules, corpus selection, coverage novelty, search RNG state |
| Executor (Rust) | Runs a Python test session under the stated process guarantees and returns bounded outcomes. | Child launch, control transport, deadlines, OS process trees, output drainage, worker replacement |
| Artifact store (Rust) | Preserves originals, derived cases, and their provenance and exposes replayable references. | Layout, version checks, atomic writes, corpus records, report serialization |
| Test adapter (Python) | Executes collected pytest/Hypothesis cases and translates their behavior into stable job outcomes. | Collection, binding, fixture scopes, stateful bridge, monitoring, Hypothesis database interaction and semantic shrinking |

The CLI selects an operation and calls the campaign; it does not implement
worker cleanup, mutate inputs, decode Hypothesis records, or write independent
report formats. Keep OS backends inside the executor and Hypothesis-version
branches inside the Python adapter.

Conceptual interface promises, to become comments beside implementation:

- `Campaign.run(selection, limits)` returns a report for all selected tests,
  including tests not reached before the budget ended.
- `Search.next_case()` produces candidate bytes; `Search.observe(outcome)`
  updates search state without inspecting Python objects or strategy internals.
- `Executor.check(test, case, budget)` returns a correlated outcome or a
  process/infrastructure event. The caller retains the case until completion.
- `ArtifactStore.record_finding(original, observation)` preserves an immutable
  original before any verification/minimization work is dispatched.
- Adapter operations cover collection, a bounded test invocation, case replay,
  and minimization. Python objects and Hypothesis database records do not become
  Rust-facing application types.

The transport implementation owns framing, version checks, message-size bounds,
and correlation IDs. Control traffic has a dedicated channel, separate from
target stdout/stderr; arbitrary target output must not corrupt the protocol.
Do not add an independent wrapper module for every protocol message or phase.

## Test execution, scopes, and scheduling

Pytest remains responsible for collection, parameter expansion, setup, and
teardown. The adapter enters fuzz execution during the item's call phase,
rather than calling a collected function without its pytest context. Collection
must detect supported Hypothesis items, including stateful tests. Non-Hypothesis
items are not converted into fuzz targets. If selection contains no supported
items, report an actionable selection error.

### Approved interpretation: finite invocations

An invocation runs many generated examples within a finite exploration slice.
Its function-scoped fixtures are set up once and torn down when that invocation
ends, not once per generated input. Returning to the same test later creates a
new invocation and new function fixtures. Module/session fixtures follow pytest
scope transitions within each worker session. Parallel workers have independent
pytest sessions; there is no cross-process shared session-fixture guarantee.

This makes equal-time scheduling possible without preserving a suspended pytest
stack for every test. Search/corpus state survives invocation boundaries. Setup
and teardown count against the campaign budget; equal search allocation cannot
promise equal case counts or that every selected test runs under a short limit.
Reports identify unvisited tests, setup overhead, and actual exploration time.

Builder decision: slice boundaries are cooperative. Stop admitting new cases
when the slice expires, allow the active case to finish within its remaining
per-case/campaign budget, then return through pytest teardown. Do not kill a
worker merely because its slice ended. Account for overrun in later allocation;
equal time is a scheduling policy, not a strict per-slice deadline. A case
timeout or campaign expiry can still force termination, in which case Python
fixture teardown cannot be guaranteed and the replacement worker starts a new
pytest session. This distinction must be visible in the report.

The alternative is retaining each test's function fixture for the whole campaign,
which conflicts with simple fair switching in a single worker. The design does
not silently promise both behaviors. The maintainer accepted this interpretation
in Q40. Verify it using fixture lifecycle counters and parameterized/stateful cases.

## Hypothesis compatibility boundary

The public `fuzz_one_input()` entry point consumes bytes, returns `None` for
rejected/overrun cases, returns canonical replay bytes for passing cases, and
saves failing cases to the configured Hypothesis database before raising.
Its ordinary generation/shrink lifecycle and settings are different from a
normal test invocation. See the [external-fuzzer guide](https://hypothesis.readthedocs.io/en/latest/how-to/external-fuzzers.html)
and [API reference](https://hypothesis.readthedocs.io/en/latest/reference/api.html#external-fuzzers).

Builder decision derived from the approved budget ownership: disable Hypothesis
`deadline` during kiryl search, replay, and semantic minimization; kiryl's
per-case timeout is the execution limit. Do not classify a disabled Hypothesis
deadline as a target failure. Normal pytest execution retains the test's own
settings, so it can raise `DeadlineExceeded` before reaching a saved assertion
failure. Report that distinction rather than claiming identical replay in
settings that stop the case earlier. Hypothesis generation-count/phase settings
do not end a kiryl campaign; test oracle semantics and explicit rejection remain
in effect. Document supported and overridden settings beside the adapter.

Fixture/parameter binding and stateful execution are feasibility gates. Current
stateful source provides an internal bridge rather than the same public fuzz
handle exposed by ordinary `@given` functions. The proposal is to pin the exact
supported Hypothesis version initially and confine any necessary internal calls
to one adapter compatibility area. Do not spread Conjecture types across Rust,
the scheduler, or persistence APIs. Unsupported versions fail explicitly.
The maintainer allowed this narrowly contained internal dependency in Q41;
its feasibility and supported version remain experiment results to establish.

External fuzz-input bytes and Hypothesis database records are different formats.
Rust owns its opaque raw corpus and artifacts; the Python adapter owns all
Hypothesis database keys, choice records, and their conversion. Test failures
must be replayed by ordinary pytest in the interoperability experiments; merely
writing raw corpus bytes into its database is invalid. Test configurations that
disable database storage must be reported as lacking automatic database replay;
explicit kiryl replay remains required. Ordinary pytest replay also remains
subject to the test's own settings.

For assertion/exception failures, prefer Hypothesis-assisted semantic shrinking
behind the adapter rather than rebuilding its strategy-aware shrinker in Rust.
Native crashes and timeouts need supervisor-controlled candidate checks. Keep
the original raw input for every finding even when a smaller semantic replay
record is also available. A derived replay representation remains opaque to
Rust; do not assume that it can be mutated as an external fuzz-input buffer.
The exact shrink bridge and mapping back to reportable arguments/operations
must be demonstrated before selecting that mechanism.

## Branch feedback and candidate engine

Use CPython monitoring as the first Python feedback candidate. Both target
interpreter versions document `BRANCH_LEFT` and `BRANCH_RIGHT`; do not assume
that a branch direction name is a portable source identity. The adapter maps
observations to identifiers valid within the recorded code/interpreter context.
Record discovered branch features rather than an unsupported coverage percentage.

Exclude kiryl, pytest, and Hypothesis execution from search feedback. Derive a
project-code scope from pytest's project context, disclose that scope, and allow
an explicit override when the intended application lies elsewhere. The adapter
owns path filtering and code identity. Rust only compares feature identities.
Monitor tool-ID allocation must coexist with other tools; do not take an occupied
ID or globally reset other tools' events.

The initial engine experiment uses a small Rust corpus-guided byte mutator:
seed initialization, elementary byte edits, and coverage-novelty retention.
Canonicalize successful buffers only through the adapter's documented result.
Measure invalid-input rate and useful executions, not just attempted mutations.
If this cannot search structured/stateful inputs effectively, revisit the engine
choice before building a generic framework or silently falling back to an
unguided implementation.

## Findings, time, and crash scope

Keep test outcome and campaign state separate. Pass/reject are not findings.
Assertions, unhandled exceptions, fatal target crashes, example timeouts, and
observed memory-limit events are findings or candidates. Collection/setup/
teardown failures, protocol errors, and unclassified worker failures are
infrastructure events. A failed fixture must not be attributed to the current
generated argument.

Before dispatch, the Rust controller retains the exact candidate and test/case
identity. When a finding is observed, atomically persist the original before
verification or shrinking. Parent-side retention allows recovery after a Python
worker crash. The proposed guarantee covers worker termination and orderly
user interruption while the controller remains able to save. Power loss,
forced controller termination, and arbitrary storage failure are not covered.
The maintainer accepted this durability scope in Q42. It avoids claiming
per-input durable storage without a corresponding synchronization cost.

Where a Python-controlled shrinker generates its own candidates, the adapter
must expose each reproducible candidate to the controller before executing it
if fatal-crash recovery is promised for that phase. This handshake and the
replay-token representation are experiment gates, not implemented capabilities.

Verification runs in a fresh worker context. A failure dependent on previous
examples or external state may not reproduce and is retained as unstable; do
not promise reconstruction of arbitrary prior external state. Minimize only
while an observable failure identity is preserved: target identity, failure
category, and stable exception/location information where available. Avoid
input-dependent exception messages as the sole identity. This identifies an
observable failure, not proof of the same underlying root cause.

All operations use monotonic remaining-budget accounting. Time waiting for IPC,
setup, execution, verification, and shrinking consumes campaign budget. Cleanup
and final artifact/report writes may continue after it. Distinguish a per-case
timeout from campaign-budget cancellation; the latter must not fabricate a
target timeout. At exhaustion, retain the finding as verification-pending or
minimization-incomplete. Ctrl-C stops new work, terminates active execution,
and attempts final persistence; do not promise success after forced termination.

## Resource control and reports

The executor exposes actual enforcement capabilities before starting work.
Reference hoimin's process-tree ownership, OS capability separation, evidence-based
OOM classification, and bounded draining output; do not infer runtime correctness
from source inspection. Linux cgroup v2 and Windows Job Objects are candidates
for hard tree limits. A portable fallback must state its weaker capabilities
and require explicit opt-in. macOS support must not claim an unenforced hard
memory cap. Backend availability, permissions, descendants, termination, and
output floods are platform experiment cases.

Builder-selected initial defaults should be centralized and documented together.
Suggested starting values are 1 GiB per worker tree, 1 MiB combined retained
stdout/stderr, a 30-second per-case timeout, and a 60-second exploration slice.
These are proposed defaults, not measured recommendations or approved behavioral
requirements. Feasibility experiments can justify changing them before adoption;
record the chosen values and evidence with their owning contract.

The artifact store owns a versioned local namespace, original/derived lineage,
target identity, required environment versions, findings, and JSON schema.
Hypothesis maintains its own database through the adapter. Do not implement
migrations. Detect unsupported data versions and request a fresh namespace
instead of deleting old findings automatically or pretending to replay them.
Human-readable argument/operation rendering is bounded and best effort; opaque
replay remains available when values cannot be represented as Python literals.

CLI operations are conceptually `run`, `replay`, and `minimize`; exact option
spelling is left to implementation design. Reports contain selected/executed/
unvisited tests, attempted/accepted/rejected cases, exploration time, observed
branch features, enforcement capability, bounded logs, failure classification,
verification/minimization status, and replay instructions. Exit-code values
must distinguish no-findings, findings/candidates, infrastructure failure, and
interruption; their numeric allocation is a builder decision. No GUI or account
integration is planned.

## Required experiments and acceptance

The next implementation plan starts with bounded, disposable feasibility work;
it must not install an engine as an established dependency or claim the entire
product is feasible merely from reading its APIs.

| Gate | Required evidence |
| --- | --- |
| Pytest/Hypothesis bridge | Fixture scope counters, parameter isolation, methods, stateful execution, expected rejection, and ordinary pytest failure replay with database storage enabled |
| Shrink/replay bridge | Ordinary assertion and stateful failures produce inspectable smaller cases; preserve original identity; native crashes/timeouts replay independently; budget exhaustion leaves usable originals |
| Python coverage | Correct application branch attribution on 3.14/3.15; no framework-driven novelty; coexistence checks with pytest-cov/monitoring tooling |
| Process safety | Active raw input survives worker death; timeout/cancel/memory events are distinguished; bounded combined output drains under floods; descendant behavior matches stated backend guarantees |
| Scheduling | Multiple tests receive equal-time opportunities; one failing test does not halt others; no concurrent execution of one test; finite invocations preserve the approved fixture semantics |
| Search value | Public planted bugs are found, feedback affects input selection, rejection/useful-execution rates and discovery times are recorded against ordinary Hypothesis |
| Platform matrix | Native macOS/Linux/Windows with CPython 3.14/3.15 GIL builds; an unrun cell remains unverified |

If a gate fails, record the observed limitation and revisit the mechanism or
requirement. Do not silently drop stateful tests, Windows, or Python 3.15.
Do not use proprietary examples or investigation findings as repository evidence.
No runtime or performance result has been obtained for this design.

## Detailed decisions approved in the interview

1. Q40: finite invocations recreate function-scoped fixtures between time slices;
   module/session scopes are per Python worker and follow pytest transitions.
2. Q41: a version-pinned Hypothesis internal compatibility bridge is allowed if
   required for stateful execution, binding, or semantic shrinking.
3. Q42: input preservation covers worker failure and orderly interruption, not
   sudden controller death or machine power loss.

## Builder review

A fresh agent reviewed this proposal for planning gaps on 2026-10-31. It raised
deadline ownership and active-case handling at slice boundaries. The decisions
above resolve these using the approved budget rule and cooperative switching.
The maintainer subsequently accepted the three detailed decisions in Q40-Q42.
This review inspected the document; it did not execute an adapter, validate a
platform, or verify performance. After the complete design was presented, the
maintainer approved proceeding to implementation planning.

## What remains with the builder

Choose exact module/file layout, transport codec and OS handle plumbing,
mutator details, test durations/seeds, CLI spelling, numeric exit codes, and
conservative default limits. Keep these decisions local to their owners and
record evidence for consequential changes. Publishing/deployment choices are
outside this design.

During implementation reviews, check that changing the Hypothesis version,
resource backend, or artifact format does not require unrelated modules to
change. Reserve explicit integration/design-review tasks instead of treating
the initial decomposition as permanently correct.

## Design diagnostic

Current structural assessment: **5/10 (four of eight diagnostics supported)**.
Responsibilities are describable, interfaces are smaller than the hidden
responsibilities, each owner hides consequential knowledge, and the ownership
map is understandable without implementation code.

The other four remain unproven: replacement without caller changes requires
an executable adapter contract; interface promises need reviewed comments and
tests; design review must occur in actual reviews; and sustained design
investment requires explicit implementation/review work. Writing this proposal
alone does not establish any runtime guarantees or a 10/10 implementation.

## Public sources and evidence limits

- [Issue #1](https://github.com/HayaoSuzuki/kiryl/issues/1): earlier research scope;
  the approved interview scope above supersedes suggestions where they differ.
- [Project baseline](../../knowledge/project-baseline.md): current implementation
  boundary; the design proposes future behavior.
- [Hypothesis external fuzzers](https://hypothesis.readthedocs.io/en/latest/how-to/external-fuzzers.html),
  [API](https://hypothesis.readthedocs.io/en/latest/reference/api.html), and
  [stateful source](https://hypothesis.readthedocs.io/en/latest/_modules/hypothesis/stateful.html):
  API/source inspection only, not integration-test results; retrieved 2026-10-31.
- [Hypothesis internal-interface policy](https://hypothesis.readthedocs.io/en/latest/reference/internals.html):
  version coupling must be isolated and tested.
- [Pytest runtime hooks](https://docs.pytest.org/en/stable/reference/reference.html#pytest.hookspec.pytest_runtest_protocol):
  lifecycle ownership; not proof that the proposed adapter works.
- [Python 3.14 monitoring](https://docs.python.org/3.14/library/sys.monitoring.html)
  and [Python 3.15 monitoring](https://docs.python.org/3.15/library/sys.monitoring.html):
  branch API candidates, not measured monitoring overhead.
- [Atheris README](https://github.com/google/atheris): upstream documented scope
  inspected 2026-10-31, not a kiryl compatibility result.
- [hoimin usage](https://github.com/HayaoSuzuki/hoimin/blob/542ec59c002d72fdff0137aa90113ffa59d70844/docs/usage.md),
  [resource implementations](https://github.com/HayaoSuzuki/hoimin/tree/542ec59c002d72fdff0137aa90113ffa59d70844/crates/hoimin-cli/src/resource),
  and [output implementation](https://github.com/HayaoSuzuki/hoimin/blob/542ec59c002d72fdff0137aa90113ffa59d70844/crates/hoimin-cli/src/process/output.rs):
  reference revision inspected statically; its tests were not executed here.
