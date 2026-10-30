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

# Current evidence and unresolved gates

The product crate still contains only the development scaffold. The integrated
Rust/Python architecture and full platform matrix have not been runtime-tested.
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
