# Runtime compatibility and order-independent resume plan

> **For agentic workers:** Use superpowers:executing-plans for these related compatibility corrections.

**Goal:** Accept the explicitly tested pytest 9.1.1 runtime and resume an unchanged campaign despite collection-order changes.

**Architecture:** Runtime admission owns a finite supported-version set; Hypothesis remains exactly pinned. Campaign construction assigns deterministic target indices by node ID before scheduling and namespace creation. Storage validation still checks target, source, interpreter, dependency and implementation identity.

**Tech stack:** Existing Rust, pytest/rstest/proptest, pinned Hypothesis, native coverage and compatibility CI. No new runtime dependency.

## Design and alternatives

Runtime admission currently rejects pytest 9.1.1 solely because the default
checkout uses 9.2.0. Support both explicit versions after real execution checks;
do not accept an untested major/minor range. Keep the checkout lock/default at
9.2.0. External project interpreters need no package/lock changes when already
using an admitted version. Changing an interpreter/dependency version still
invalidates a previously saved environment identity.

Campaign namespaces currently use target indices derived from collection order.
A plugin may reorder an unchanged target set, so the same index names a different
target on restart. Manifest rejection correctly protects evidence but prevents
valid resume. Canonicalize target construction by node ID before charging,
selection, deterministic seed offsets and namespace assignment. Do not disable
manifest checks, force plugin seeds or introduce another persistent ID map/hash
scheme. Stable index tie-breaking and the public pure scheduler API remain intact.
The order of equal-debt targets becomes lexicographic by node ID, documented as
an observable deterministic ordering rule.

Existing version-1 manifests and records remain unchanged. Stores created with
different implementation/source/dependency identity remain incompatible; this
is not automatic migration or permission to overwrite originals.

## Task 1: runtime admission

- [x] Observe rejection of pytest 9.1.1 with an independent parameterized regression.
- [x] Admit only 9.1.1 and 9.2.0, retaining exact Hypothesis and interpreter guards.
- [x] Run real pytest 9.1.1 product tests in an isolated public dependency environment.
- [x] Add native compatibility execution on all six OS/Python cells after default coverage collection.

## Task 2: stable target construction

- [x] Add typed target-construction and actual reordered-collection resume regressions; observe failures.
- [x] Canonicalize target IDs at campaign construction without modifying caller input or identity checks.
- [x] Add generated permutation invariance and literal ordering/empty cases.
- [x] Run full local suites, strict/formal/fuzz checks and instrumented coverage.
- [x] Obtain independent read-only review and address findings in one pass.
- [x] Update user guidance/current contracts for supported versions and canonical ordering.

## Five self-reviews per phase

| Pass | Design | Plan | Implementation check | Test check |
| --- | --- | --- | --- | --- |
|1 ownership|Admission and indexing have explicit owners|Keep fixes bounded|No caller-side seed workaround|Known supported/rejected versions and reordered collection|
|2 compatibility|Finite version set, no blanket range|Real alternate-runtime execution|Hypothesis and GIL guards unchanged|Full suite actually imports pytest 9.1.1|
|3 identity|Canonical index represents node ID|Preserve schema/manifest checks|No bypass or deletion of old records|Same environment plus same target set across order changes|
|4 scheduling|Stable lexical ties, same debt policy|Use existing TargetProgress owner|No new scheduler/storage abstraction|Literal expected order, permutation property, Lean/fuzz|
|5 acceptance|Retain conservative mismatches|Docs and full native checks|Self-contained authored regression fixtures|Generic authored regressions, six-cell exact gate|

The persistent indexing boundary initially satisfies 6/8 diagnostics (7.5/10):
implementation isolation and knowledge ownership fail because namespace meaning
leaks through a plugin-controlled collection position. Isolate persistent indexing
from that detail. The runtime guard was deliberately conservative; broaden its
explicit supported-version contract only with execution evidence. Final scoped
score requires independent implementation review, not this plan alone.

## Revision-scoped implementation evidence

This checkpoint is the working-tree implementation based on dd424b4, before
publishing its own commit. All five design/plan reviews selected finite version
admission and canonical construction instead of loosening checks, forcing plugin
seeds or adding a namespace migration/map framework. Implementation reviews
checked the admitted versions, pre-import guards, exact IDs, canonical indexing,
unchanged storage checks and the compatibility step's actual interpreter.
The five test reviews covered explicit versions, actual CLI reorder lifecycle,
permutation/literal oracles, independent formal/fuzz constraints and exact native
acceptance rather than inferring it from configuration.

The authored runtime regression rejected 9.1.1 before the fix. The authored CLI
regression returned storage identity mismatch when only collection order changed.
After the fix, the default runtime's 13 focused cases pass, and the actual
pytest 9.1.1 interpreter passes all 286 product Python tests (six native-only
skips), plus all 13 focused cases after final oracle refinement. A budget may
legitimately return an unresolved candidate (exit 1); the lifecycle test admits
0/1 while independently requiring searched statuses, unchanged identity, positive
attempt counts, canonical IDs and both corpus snapshots. Infrastructure or actual
terminal failures cannot pass those assertions.

Full Rust tests pass 395 cases; strict host/Linux GNU/Windows GNU Clippy,
217 formal scheduler rows, fuzz compilation, rustdoc, workflow lint and 28 document
tests pass. Local instrumented coverage collection succeeds with the full product
suites; platform-only locations still require the six-cell native union.

Independent read-only review found no Critical or Important implementation issue
and passes all eight persistent-index diagnostics (10/10 scoped). Its Minor
finding concerned stale plan checkboxes; this checkpoint records completed local
steps with evidence rather than claiming future native success. The reviewer
declined native execution, parent-owned local execution, pending documentation
and migration of incompatible stores. Parent checks own execution; this contract
explicitly retains incompatibility rejection and has no migration promise.

## Final acceptance gate

Publish the implementation and update PR #6 with only these self-contained
fixtures and public runtime results. Require all current native jobs, including
the six actual pytest 9.1.1 compatibility phases, and the exact default-profile
coverage union. The completion ledger and named-revision PR checks own final
acceptance; this pre-publication checkpoint is not an invented native result.

## Native harness reliability follow-up

At 023e63e, parallel delegated coverage failed before a native fixture's protocol
started: launching its freshly written executable returned ExecutableFileBusy.
The normal Rust phase at the same revision passed. Delegated Linux coverage now
runs Rust harness cases with RUST_TEST_THREADS=1, reducing interference between
mutable launcher fixtures. Every case, including delegated ignored cases and
explicit within-scenario process concurrency, remains in measurement scope.
No retry, ignored case or coverage exclusion is added.

A separate experimental blocked-writer check used a 700 ms campaign limit, which
could expire during Windows startup before any active input existed. A controlled
one-second fixture setup reproduced the missing-original failure locally. Its
campaign window is now eight seconds, below a twenty-second case limit, while
the artificial blocked receive lasts sixty seconds. The unchanged exact-buffer
assertion passes after the fix (8.15 s locally). The external fifteen-second
subprocess watchdog still detects an indefinitely blocked parent. These changes
adjust test preconditions, not product deadline or storage policy.

## Replay-probe timing follow-up

At 7990f98, every native product job, including real pytest 9.1.1 phases and the
exact coverage union, passed. A separate macOS experiment produced unstable raw
crash replay with a 150 ms case limit. A marker-controlled 300 ms delay on fresh
synthetic crash execution reproduced the same unstable result locally before
the fix. The raw replay table now distinguishes crash and hang budgets and also
requires their literal native_crash/case_timeout labels, so merely confirming the
wrong kind cannot pass.

Positive finite-input probes have no unrelated default global deadline; their
case allowance is five seconds and the external subprocess watchdog stays fifteen
seconds. Tests of global/case expiry specify their own limits. Case expiry and
campaign expiry are separate invocations rather than two unrelated cases in one
function. A controlled four-second setup reproduced missing pending-original
evidence under the old three-second budget; an eight-second window plus a long
teardown now verifies actual post-failure cancellation.

The final full experiment suite passes 185 tests with six native-only skips;
strict Ruff ALL, formatting and ty pass. An independent read-only review of this
bounded test diff found no Critical, Important or actionable Minor issues and
confirmed the strengthened fault-kind, replay, liveness and pending-evidence
oracles. It declined pending native execution, freedom from arbitrary host delays
and descendant cleanup after an externally killed malfunctioning parent; the
latter is unchanged. Product execution source is unchanged by these fixture-only
corrections; final head checks still own whole-PR acceptance.
