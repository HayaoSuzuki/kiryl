# Target-owned scheduling implementation plan

> **For agentic workers:** Use superpowers:executing-plans for this one-task refactor.

**Goal:** Each target owns its cumulative exploration charge and scheduling eligibility, so the campaign cannot forget to synchronize parallel state arrays.

**Architecture:** TargetProgress records its own saturating exploration charge and derives eligibility from TargetStatus. Campaign selection reads those rows through the same least-charge primitive used by the public formal/fuzz oracle adapter. Keep the public choose_next slice API and its missing-status rule unchanged.

**Tech stack:** Existing Rust, rstest/proptest, Lean correspondence and native test-inclusive coverage. No dependencies.

## Design

At cd29a22, run maintains charged and stopped vectors alongside TargetProgress.
record accepts an externally accumulated charge and returns a stopped flag, while
also writing its own status and serialized charge. A caller must update both
representations in the correct order. The actual rule is cumulative saturating
addition of max(exploration_ms, 1), with terminality determined by stops_search.
Only exploration, including cooperative overshoot, creates debt; replay/shrink
must not be charged. Errors retain previous charge and counters.

Keeping the arrays preserves that coordination cost. A new scheduler object or
an identifier map adds interfaces and risks changing stable index tie-breaking.
The existing target row already owns the necessary data: let it own updates and
eligibility too. Selection shares one minimum-by-charge primitive with the pure
public adapter; generated model rows and scheduler fuzzing remain authoritative.

Initial diagnostics: 6/8 (7.5/10). Implementation isolation fails because run must
coordinate accounting and terminality; knowledge ownership fails because target
state is represented in multiple mutable places. The other six rows pass in
this scoped campaign boundary. No whole-repository score is claimed.

## Task 1

- [x] Add typed-row selection/accounting tests; observe compiler-red against the old interface.
- [x] Move charge and eligibility ownership into TargetProgress and remove parallel mutable arrays.
- [x] Preserve the public selector adapter and share its least-charge primitive.
- [x] Add independent status/minimum-charge cases, saturation properties and typed-row/oracle correspondence.
- [ ] Run full Rust/strict checks, 217 generated formal rows, fuzz compile and local coverage.
- [x] Obtain independent read-only review and resolve actionable findings in one pass.
- [ ] Update the existing architecture contract and source hashes; commit and update PR #6.
- [ ] Verify all native checks and exact test-inclusive coverage for the current revision.

## Five self-reviews per phase

| Pass | Design | Plan | Implementation check | Test check |
| --- | --- | --- | --- | --- |
|1 ownership|One target owns status and charge|Change record and scheduler together|No parallel charged/stopped state in run|Typed row status/charge assertions|
|2 accounting|Only exploration creates debt|Preserve max-one and saturating addition|Read exploration_ms before moving report|Zero, positive, maximum and generated charges|
|3 eligibility|Status determines eligibility|Retain outcome stopping policy|No extra mutable boolean|All four target status variants|
|4 oracle|Stable index ties and missing-status rejection|Share the selector primitive|Keep public choose_next signature|Generated typed-row/public-adapter agreement, Lean and fuzz|
|5 acceptance|No new scheduler framework|Preserve report/storage/CLI semantics|Errors preserve accumulated state|Full suites, independent review, six-cell exact gate|

User guidance needs no change: exploration allocation, replay/shrink exclusion,
report schemas, CLI options and persistence guarantees keep their existing meaning.
Current native results must be recorded against their tested revision in the PR;
old coverage is not evidence for the changed implementation.

## Review and local evidence

All five design and planning reviews selected one existing target owner and
rejected another scheduler abstraction or changing identifier/index semantics.
The implementation reviews checked record ownership, max-one saturating debt,
status eligibility, stable index/seed behavior and preserved error snapshots.
The test reviews checked literal status/charge cases, generated saturation,
replay/shrink exclusion, typed/public correspondence and native acceptance scope.

The new accounting/selection API failed compilation before implementation.
After implementation the full Rust suite passes 391 cases. Generated Lean rows
match in 217 cases, the fuzz manifest compiles, and rustdoc plus 28 documentation
tests pass. Strict host/Linux GNU/Windows GNU Clippy found a boolean-match style
issue in the generated-row fixture; it uses the standard conditional now, outside
any test function body. No lint or coverage exclusion was introduced.

Independent read-only review found no Critical, Important or Minor issues;
all eight scoped design diagnostics pass (10/10). It confirmed charge/eligibility,
seed conversion/wrapping, ordering, output namespaces and error-break behavior
match cd29a22. The typed/public property checks representation and eligibility
mapping; it cannot independently reject a defect shared by the least-charge
primitive. Existing literal tests, Lean-generated expectations and fuzz assertions
provide that independent selection oracle.

The reviewer declined executing the verification commands, native exact acceptance,
pending documentation hashes, whole-repository architecture and measured time
allocation. Parent verification owns execution and refreshed sources; current PR
checks own native evidence. No whole-repository or measured time-percentage score
is inferred. Final local instrumented/native results remain revision-specific.
