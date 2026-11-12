---
type: Decision
title: Rust controller and separate Python test workers
description: Approved Rust/Python architecture direction and planned feasibility gates.
status: draft
sources:
- id: proposal
  resource: ../work/coverage-guided-python-fuzzing/design.md
  revision: fed3bcaabbc36c2ddee3d2c6c4858ca1803057fd
- id: plan
  resource: ../work/coverage-guided-python-fuzzing/feasibility-plan.md
  revision: 7bf03ada6ace3ad196eb97fbb4d3f54dd09ea27d
- id: crate
  resource: ../../../src/lib.rs
  revision: 5c4eccb43708bc115c83bc2f468c22758d73b5e1
- id: product-campaign
  resource: ../../../src/campaign.rs
  working_tree: modified
  sha256: 99c1502cc758ac43ea56532cf8fb12f8667fb24881feb73ba2088a56e4c35660
- id: product-executor
  resource: ../../../src/executor.rs
  working_tree: modified
  sha256: 87803b8cf069e52007c819e7fdcfbf39de98f17765d63e329eec64802058aa21
- id: product-store
  resource: ../../../src/artifacts.rs
  working_tree: modified
  sha256: 07b82576503bcce0cb90cb8ef0d41a6fc76f7453aed717c7797304a4aa3c2fd0
- id: product-python
  resource: ../../../python/kiryl_adapter/worker.py
  working_tree: modified
  sha256: 1f119992b5514277a8ea165de0d6d6cc44cce5021ce9604189c0a8ddfc6e9690
- id: product-report
  resource: ../../../src/report.rs
  working_tree: modified
  sha256: 6aa680917862fb370422dcfc221501e1dae95a5958bb803698792a80d197dc98
- id: product-command
  resource: ../../../src/command.rs
  working_tree: modified
  sha256: b3278305a0bd5c39405204490028888d5ae204eca7a6207cc9dc334b6412c3f6
- id: product-verification
  resource: ../../../src/verification.rs
  working_tree: modified
  sha256: c3530e563d93ae709f4b0b98129b86ab8a7e28cf3bb0eb6e938820108d4727e6
- id: target-scheduling
  resource: ../work/product-cli/target-scheduling-plan.md
  working_tree: untracked
  sha256: 9e162cebb1e64ef1aea74727b4ccc222e351748fd74d489724bdb4123db305f6
- id: presentation-policy
  resource: ../work/product-cli/presentation-policy-plan.md
  working_tree: untracked
  sha256: f8e105b6eba95b5865e8fa2b8106f0b689e35f1bedeec556ed49e75767d40ee7
- id: session-operation
  resource: ../work/product-cli/session-operation-design.md
  working_tree: untracked
  sha256: e6bcf01748be7e633bc2c3aa2bcfcf9d0baa9a938ace5382729b32c956d5ebdd
---
# Selected direction

The maintainer approved the product scope and selected architecture A in the
2026-10-31 specification/design conversation: a Rust controller generates
candidate inputs and drives separate Python test workers. The
[approved design](../work/coverage-guided-python-fuzzing/design.md) owns the recorded
requirements, alternatives, detailed responsibilities, and outstanding experiment
gates. After reviewing its detailed choices, the maintainer instructed the agent
to proceed to implementation planning. This approval establishes no runtime
capability. The maintainer also accepted finite invocations, a contained
version-pinned internal Hypothesis bridge when needed, and worker-failure rather
than machine-failure durability in Q40-Q42.[^proposal]

# Information boundaries

The Rust side owns campaign allocation, candidate generation, worker/process
control, and artifact persistence. The Python adapter owns pytest lifecycles,
Hypothesis-specific data and APIs, stateful integration, and Python branch
measurement. Parent-side retention of an active candidate makes worker-crash
recovery possible without placing the search state inside the failing Python
process. This remains a proposed mechanism requiring experiments.[^proposal]

Compared with a Rust search extension inside every Python worker, the selected
direction avoids that extension's build/ABI requirements and keeps crash state
in the controller. Per-case IPC overhead is the corresponding cost. Reconsider
the placement only with measurements showing that this cost warrants a more
complex integration.[^proposal]

# Historical feasibility baseline and unresolved gates

At the historical crate revision cited below, the product contained only the
development scaffold. Product implementation and newer local observations are
recorded in the product follow-up below; full native acceptance requires actual
CI evidence rather than this earlier baseline.
The isolated Python experiment now has limited evidence described in the
[fixture lifecycle audit](fixture-lifecycle-audit.md) and
[Hypothesis replay audit](hypothesis-replay-audit.md), and
[Python coverage audit](python-coverage-audit.md); these do not establish the
whole product's compatibility.[^crate]

The first-version acceptance targets include all three operating systems and
both specified CPython versions, fixtures/parametrization/stateful testing,
branch-guided discovery, replay/shrinking, and honest process-limit reporting.
The detailed proposal identifies the exact matrix, experiment gates, proposed
fixture slicing, possible version-pinned Hypothesis internals, and durability
scope. Engine selection remains open. Source inspection alone is insufficient
to declare any gate passed.[^proposal]

The [feasibility implementation plan](../work/coverage-guided-python-fuzzing/feasibility-plan.md)
orders six experiment tasks: pytest binding, stateful replay/shrinking, isolated
branch feedback, Rust supervision and artifacts, OS resource capabilities, and
guided-search/platform evidence. The maintainer selected inline execution with
five self-reviews per task, Lean, property/fuzz testing, and strict Python checks;
writing it does not adopt a production engine or complete product acceptance.
Dependent product campaign/CLI work follows the evidence gate.[^plan]

# Reconsideration conditions

Revisit this direction when adapter experiments cannot preserve the approved
test semantics, an engine cannot cover the required runtime matrix, mutation
produces mostly unusable inputs, measured IPC dominates useful execution, or
process control cannot deliver its disclosed guarantees. Do not silently narrow
the product scope to make a candidate pass.[^proposal]

[^proposal]: [Coverage-guided Python fuzzing design and recorded interview decisions](../work/coverage-guided-python-fuzzing/design.md).
[^plan]: [Feasibility implementation plan](../work/coverage-guided-python-fuzzing/feasibility-plan.md).
[^crate]: [Scaffold library](../../../src/lib.rs), inspected at `5c4eccb43708bc115c83bc2f468c22758d73b5e1`.

# Product implementation and acceptance limits

The product now exposes `run`, `replay` and `minimize` from a source checkout.
Rust controls bounded search/corpus, equal exploration-time allocation, worker
supervision and immutable original persistence. Separate Python modules own
pytest collection/lifecycles, the pinned Hypothesis bridge and branch monitoring.
This adopts the experiment's byte-mutation direction because it met the recorded
feasibility gates without a Python extension ABI or an external engine dependency.
No generic multi-engine interface or Atheris dependency was introduced.[^product-campaign][^product-executor][^product-python]

A single writer owns the campaign root. Per-target namespaces compare a versioned
manifest containing target variant and caller-generated environment/source/config
identity. Unsupported schemas and detected mismatches reject reuse without deleting
originals. Corpus snapshots are bounded at 256 entries of up to 4096 bytes; original
findings are never evicted with exploration corpus. Startup can leave conservative
locks after process death; manual inspection is required before removal.[^product-store]

Collection uses an attached child before importing project code. Worker control
uses reserved descriptors independently of pytest capture and target output. Cases
start timing after setup; campaign wall time includes collection/setup/verification.
Fresh workers confirm fingerprints; interrupted shrink evidence is persisted as a
separate pending derivative before confirmation. No confirmation follows merely
from shrinking completion.[^product-executor][^product-python]

The [implementation plan](../work/product-cli/implementation-plan.md),
[storage reviews](../work/product-cli/storage-review.md),
[acceptance evidence](../work/product-cli/acceptance-review.md), and
[Python reviews](../../../python/REVIEW.md) record scoped local checks and remaining
gates. Native macOS CPython 3.14 evidence does not establish all six matrix cells,
Linux delegated-cgroup enforcement, Windows OOM observability or long-term performance.
The product CI executes real CLI scenarios on the intended matrix; unexecuted jobs
are not acceptance evidence. The [user guide](../../user/index.md) owns commands and
actual prerequisite/limit explanations.

Reconsider the adopted direction if replay semantics, corpus usefulness, bounded
memory or measured IPC cost no longer satisfy the intended workload. Do not weaken
resource or reproduction guarantees to count a platform as accepted.

[^product-campaign]: [Product campaign](../../../src/campaign.rs).
[^product-executor]: [Native executor](../../../src/executor.rs).
[^product-store]: [Artifact store](../../../src/artifacts.rs).
[^product-python]: [Python worker](../../../python/kiryl_adapter/worker.py).

The [integration review](../work/product-cli/integration-review.md),
[finite boundedness observation](../work/product-cli/boundedness-review.md), and
[ordinary Hypothesis comparison](../work/product-cli/benchmark-review.md) retain
review findings, negative observations and finite-run measurement limits. Finding
presentation and machine-readable error reports now have a dedicated Rust report
owner; unknown collection completion is kept explicit instead of preserving a
stale successful report.[^product-report]

[^product-report]: [Finding presentation](../../../src/report.rs).

The [final native matrix](../work/product-cli/evidence/native-matrix.json) records
successful product CLI execution on all six intended OS/interpreter cells and
separate hard-resource gates at source revision `795fe6e`. Platform-specific
skips do not stand in for required hard enforcement; those four jobs ran without
best effort. The acceptance record owns the exact checks, limits and snapshots.

# Storage and completion boundary refinement

ReplayContext now owns manifest decoding, recorded selection, environment admission
and campaign lock lifetime. Execution consumes typed SavedFinding evidence rather
than the original envelope; presentation consumes FindingDescription. These
boundaries allow storage representation changes without CLI, execution or display
JSON-path edits.[^product-store][^product-executor][^product-report]

Report owns Completion and shared exit/target-stop classification. CLI and campaign
no longer repeat exit-status decisions. The [refactor review](../work/product-cli/boundary-refactor.md)
records alternatives, five passes per phase and the independent scoped diagnostic
assessment. CLI usage and version-1 storage encoding are unchanged, so user commands
require no update. Revisit these boundaries if another consumer must decode stored
JSON or invent its own completion precedence.[^product-report][^product-campaign]

# Typed campaign progress

Campaign counters and snapshots are now private typed state, serialized only at
the report boundary. TargetProgress distinguishes cumulative measurements from the
latest slice and preserves accumulated values on infrastructure failure. JSON
field names no longer participate in runtime aggregation. Scheduling selection
and public report field presence remain unchanged.[^product-campaign]

LogTail owns observed-byte accounting and bounded merging for both worker sessions
and campaign slices. Its fields are private; callers supply streams rather than
reimplementing truncation and saturation. The [aggregation review](../work/product-cli/campaign-aggregation-refactor.md)
records the alternatives, compatibility assertions, regression/property tests and
scoped review results. Revisit this boundary if another consumer must read back
serialized progress or recompute stream accounting.[^product-executor]

# Product command ownership

Command owns discovery admission, remaining wall-time budget, saved-finding
compatibility and final report construction. Its result retains replay's writer
lock through publication. CLI owns syntax and signal installation; it no longer
decodes collection fields or configures experimental worker modes. Discovery
hides the Python wire envelope and source identity encoding; executor accepts
only guided exploration or typed replay/minimize. Unused product baseline/fixed
modes were removed without changing isolated experiments or version-1 output.
The [command review](../work/product-cli/command-boundary-review.md) and
[executor review](../work/product-cli/executor-boundary-review.md) record reasons,
alternatives and five passes per phase. Reconsider if another caller needs to
know discovery fields or manage the saved-finding transaction lifetime.
User arguments and guarantees are unchanged; no user-guide change is needed.
[^product-command][^product-executor][^product-report]

[^product-command]: [Product command transaction](../../../src/command.rs).

# Fresh reproduction policy

FailureIdentity and Verdict now own reproduction matching and incomplete-check
classification across automatic replay, reduced-candidate checking and standalone
operations. Executor retains fresh process, evidence persistence and acknowledgment
sequencing; output strings no longer decide whether automatic shrinking may start.
Automatic timeout confirmation compares kind alone; standalone raw replay retains
exact rendered-detail matching and the version-1 semantic alternative. Skip/reject
remain distinct standalone labels but imply an unstable candidate. Missing checks,
interruption, campaign expiry and infrastructure never confirm reproduction.
The [policy plan and review](../work/product-cli/verification-plan.md) record
alternatives, five reviews per phase and compatibility tests. No CLI, storage or
wire-format changes require user-guide updates. Revisit this policy if outcome
kinds or saved-record matching rules change; do not reintroduce copies in callers.
[^product-verification][^product-executor]

[^product-verification]: [Fresh reproduction policy](../../../src/verification.rs).

# Test-driven boundary corrections

Coverage work replaced command's independently optional replay context and mode
with a private PreparedOperation, retaining the admitted context and writer lock
as one state. ReplayContext retains its validated UTF-8 original representation
so callers do not repeat an impossible encoding failure. JSON Value persistence
uses infallible formatting after arbitrary serialization has already completed.
Unix notification decoding and interrupted-poll policy are independently testable
without reaping the cleanup anchor. These remove impossible states and expose
real OS/transport errors to tests rather than excluding whole modules.
The [test quality contract](test-quality.md) owns the new testing/measurement
requirements; public CLI and version-1 artifacts remain unchanged.
[^product-command][^product-store][^product-executor]

ValidatedOriginal now carries its checked filename with the decoded envelope;
artifact callers no longer repeat impossible filename/encoding failures. Spawn
pipe admission and collection-event decoding share private policies; resource
observations and cleanup outcome recording can be tested without fabricating OS
handles. Linux discovery parsing and attachment fallback are independently tested,
while native delegation remains a separate execution requirement. The test work
record owns the measured revision and remaining coverage, not a blanket correctness
claim.[^product-store][^product-executor]

The private session operation couples replay/minimization with their required
opaque token. Empty tokens remain valid; missing semantic tokens cannot be
constructed. This boundary owns adapter start-frame encoding, while standalone
reproduction owns its public report labels. Raw replay still uses run on the
worker wire and dispatches its retained input once. Explicit tuple matching of
frame and operation preserves admission and correlation without string-mode
knowledge in callers. Reconsider the enum when a real new operation needs a
different payload; do not add independent nullable control fields.[^session-operation][^product-executor]

[^session-operation]: [Typed session operation design and verification](../work/product-cli/session-operation-design.md).

[The runtime-state contract](runtime-state.md) owns the broader enum/type boundaries, diagnostics and unknown-label compatibility for both implementations.

Reproduction command rendering receives the original requested Limits directly.
The command transaction snapshots it before collection-time budget subtraction;
report JSON contains display fields and never selects memory, case-timeout or
best-effort options. A private borrowed presentation context owns command encoding
for standalone and nested campaign findings. Its Rust helper API requires the
typed policy, while actual CLI arguments, report/storage schemas and replay-lock
lifetime retain their contracts. Reconsider the policy inputs when a new replay
option is implemented; do not reintroduce display-key configuration lookups.
[^presentation-policy][^product-command][^product-report]

[^presentation-policy]: [Typed presentation policy design and verification](../work/product-cli/presentation-policy-plan.md).

Campaign target rows own cumulative exploration debt and scheduling eligibility.
Recording a slice saturating-adds max(exploration_ms, 1), preserves the bounded
latest snapshot and derives eligibility from typed target status. The campaign
reads the same debt for selection and deterministic seed offsets; it no longer
maintains parallel mutable charge/stop arrays. Replay and shrinking never create
exploration debt. Error snapshots preserve accumulated counters and charge.
The typed scheduler and public pure selector share one stable least-charge
primitive. Generated typed/public correspondence connects representations, while
literal tests, Lean cases and fuzz invariants independently constrain selection.
Revisit this ownership when a real new target state or charging rule is introduced;
keep one stored source of truth and the existing report/CLI contract.
[^target-scheduling][^product-campaign]

[^target-scheduling]: [Target-owned scheduling design and verification](../work/product-cli/target-scheduling-plan.md).
