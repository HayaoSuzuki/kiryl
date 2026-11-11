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
  sha256: 37ad37b04039cc7e2322b4893406b3ef4abc7319574b069ace528d77e06e99d4
- id: product-executor
  resource: ../../../src/executor.rs
  working_tree: modified
  sha256: bbaeba04362c28725c482f396902768771f1932215e9a16d63082042399f87c2
- id: product-store
  resource: ../../../src/artifacts.rs
  working_tree: modified
  sha256: 6c7e089cb13203ce8dd13483ad0f08f36352be5598cd0ed94b671f4e5beb0e0e
- id: product-python
  resource: ../../../python/kiryl_adapter/worker.py
  revision: 6d41684763c38d7c649cc32a29b8306d89229fa4
- id: product-report
  resource: ../../../src/report.rs
  working_tree: modified
  sha256: 14643e5d55f489fb0252c186cbd1e66f3a8d500b3a345643c81c1c1b5db6672e
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
