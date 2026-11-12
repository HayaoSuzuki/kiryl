---
okf_version: "0.2"
---
# Development knowledge

Choose the topic relevant to your change, then inspect its cited sources.
These initial concepts are drafts; metadata and link checks do not establish
implementation correctness or human review.

- [Documentation ownership](documentation-policy.md): Audience boundaries,
  canonical topics, and evidence without duplicated current explanations.
- [Project baseline](project-baseline.md): Rust and licensing choices, current
  scaffold, and engine-selection limits.
- [Fuzzing architecture direction](fuzzing-architecture.md): Selected Rust/Python
  boundary, approved design, and compatibility experiments still required.
- [Fixture lifecycle audit](fixture-lifecycle-audit.md): Native Python experiment
  observations, Lean-model correspondence, and remaining limits.
- [Hypothesis replay audit](hypothesis-replay-audit.md): Stateful/ordinary database
  replay, semantic shrinking, budget checks, and unverified boundaries.
- [Python coverage audit](python-coverage-audit.md): Per-case application feedback,
  monitoring ownership, coexistence limitations, and measured component overhead.

- [Rust supervision audit](rust-supervision-audit.md): Native process recovery,
  bounded transport, original persistence and budget/model limits.

- [Resource capability audit](resource-capability-audit.md): Startup attachment,
  fallback limits and native enforcement evidence still required.

- [Guided discovery audit](guided-discovery-audit.md): Fixed-budget observations,
  isolated baselines, model limits and pending native acceptance.

Use the [documentation procedure](../documentation.md) to maintain this bundle
and the [development guide](../index.md) to run checks.

- [Test quality](test-quality.md): Independent cases, generated properties, strict Python checks and exact native coverage.

- [Typed runtime states](runtime-state.md): Enum ownership, boundary encoding and unknown-label compatibility.
