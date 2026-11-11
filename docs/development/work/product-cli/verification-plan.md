# Reproduction policy implementation plan

> **For agentic workers:** Use superpowers:executing-plans for inline execution.

**Goal:** Give one owner to fresh reproduction matching and uncertainty rules.
**Architecture:** A private verification module classifies actual observations
against an explicit failure identity. It distinguishes semantic fingerprints,
kind-only timeout matching and native/raw kind-plus-detail matching. The executor
owns process and persistence sequencing, consuming verdicts without repeating rules.
**Tech Stack:** Existing Rust/serde; no new dependencies.
**Spec:** Design and review below.

## Design

The current executor repeats pending/confirmed/unstable rules five times. Keeping
these in procedural helpers still leaves identity knowledge at every site. A
private policy module instead owns matching and verdict representation. A generic
callback/predicate framework adds unnecessary choices and is rejected. Distinct
standalone skipped/rejected labels remain; candidate presentation maps both to
unstable. A matching verdict never follows merely from completing shrink work.
Native exit detail, timeout classification, absent observations, cancellation,
interruption and infrastructure must retain their existing treatment.

## Constraints and review focus

- Preserve version-1 JSON, pending writes before confirmation and immutable originals.
- Preserve missing fingerprint Option equality for compatible stored evidence.
- No classifier may confirm cancellation, missing observations or infrastructure.
- A changed native exit must remain unstable; automatic timeout ignores rendered text.
- Standalone skip/reject labels differ deliberately from candidate confirmation.

## Steps

- [x] Add table-driven policy tests covering exact identities, mismatches, absent
  observations, interruption, cancellation, infrastructure, skip and reject.
- [x] Observe failure before implementing classification.
- [x] Implement FailureIdentity and Verdict; keep only current matching modes.
- [x] Replace all five executor classifications without changing write order.
- [x] Run Rust and actual CLI Python suites, strict quality/docs checks.
- [x] Fresh independent integration review, fix findings and update OKF. PR update/native CI are the completion gate
  recorded in the PR rather than an invented future result in this source snapshot.

## Self reviews before implementation

Design passes: (1) policy belongs together rather than by execution phase;
(2) identity modes cover all current uses; (3) skip/reject presentation is explicit;
(4) pending evidence ordering remains executor-owned; (5) no new public/library API.
Plan passes: (1) every rule has a table case; (2) five callers traced; (3) no Python
wire change; (4) whole CLI/native checks retained; (5) fresh review and PR completion
included. Ruling: continue inline on the existing isolated work/product-design
branch under the user's repeated authorization to finish implementation and PR.

## Implementation reviews

1. Traced all five previous policy expressions to replacement calls; persistence,
   fresh-worker launch and acknowledgment ordering are unchanged.
2. Checked differences deliberately retained: automatic timeout matches kind;
   standalone raw matches detail; saved raw retains semantic-or-raw compatibility.
3. Checked missing fingerprint equality rather than silently strengthening admission.
4. Checked pending precedence before matching and standalone versus candidate labels.
5. Clippy rejected nested Option representation; replace it with explicit semantic
   identity states, keeping wire strings only at serialization-facing status methods.

## Test reviews

1. Confirmed missing policy types produced the planned RED; new tests then passed.
2. Table checks matched and mismatched fingerprints, including same fingerprint on pass.
3. Native tests check changed exit detail and automatic timeout detail independence.
4. Saved-identity tests pin standalone detail matching and rejected label.
5. Existing real CLI tests exercise immutable original, raw replay, fresh semantic
   replay/minimize, cancellation and resource paths; native CI must rerun on new revision.

## Independent review and completion evidence

The fresh reviewer compared all five replacements against `02d2774` and inspected
CLI/command/executor/Python integration. No Critical/Important regression was found.
Two Minor findings were addressed: constructor contracts now explicitly distinguish
automatic versus standalone timeout rules, and saved raw absent-fingerprint
cross-kind matching has a compatibility assertion. The reviewer rechecked both
and reported no outstanding finding. Typed Verdict now gates shrinking directly.

Initial policy score: 6/8 diagnostics (7.5/10), failing implementation independence
and important-decision ownership because matching was duplicated across callers.
The initial independent review passed 7/8 (8.75/10), failing interface comments.
After fixes the reviewer passed all eight (10/10) for this integrated policy boundary.
The remaining large string-driven supervision machine, Python/Rust verification
labels and untyped output JSON prevent treating this as repository-wide perfection.
They are existing complexity, not a reason to add speculative generic interfaces.

Fresh local results: 43 Rust tests; strict Clippy, fmt, rustdoc and build; 72 product
Python/CLI passes and four platform skips; 28 documentation tests, OKF/link checker
and strict MkDocs build. Native CI results for the resulting commit are recorded
on PR #6, separately from older accepted revision evidence.
