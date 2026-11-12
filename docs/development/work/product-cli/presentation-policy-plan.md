# Typed presentation policy implementation plan

> **For agentic workers:** Use superpowers:executing-plans for this one-task refactor.

**Goal:** Reproduction commands use the typed requested execution policy, independent of report JSON representation.

**Architecture:** The command transaction retains its original Limits snapshot before subtracting collection time. Presentation receives that snapshot directly. A private presentation context owns traversal and command rendering; JSON is output, not a configuration channel.

**Tech stack:** Existing Rust/serde_json, rstest/proptest, cargo-llvm-cov and the six-cell native product gate. No dependencies or CLI/schema changes.

## Design and alternatives

At 0067c62, command serializes Limits, reads allow_best_effort back from JSON,
and report reads tree_memory_bytes/case_ms by key. Missing or mistyped display
fields silently omit options. This disagrees with the intended information
boundary: the requested policy, not a mutable display document, owns replay.
The actual CLI always writes complete limits, so its observed output must remain
identical; the internal Rust presentation signature changes to require Limits.

Retaining JSON lookup leaves the coupling. Deserializing the report into Limits
adds a second source of truth and late parsing errors. Passing the typed original
snapshot makes the dependency explicit and lets compiler checking replace schema
knowledge. A private borrowed context reduces the internal annotation signature
and shares the same policy for standalone and nested campaign findings.

Scope: presentation and its command caller, with literal output and generated
policy tests. Keep original records, replay-lock lifetime, budget accounting,
exit severity, storage format and experiment policy unchanged. No user-guide
update is needed because commands and actual CLI output retain their contract.

## Task 1

- [x] Add a typed-policy regression and observe compiler-red against the old API.
- [x] Implement typed presentation and update existing direct callers.
- [x] Add parameterized policy and generated numeric-policy oracles; preserve independent literal JSON expectations.
- [ ] Run full Rust tests, strict checks, documentation checks and local instrumented coverage.
- [x] Obtain an independent read-only review; fix actionable findings in one pass.
- [ ] Refresh relevant OKF source hashes; commit and update PR #6.
- [ ] Require all native checks and exact test-inclusive native coverage; record completion against this revision.

## Five reviews per phase

| Pass | Design | Plan | Implementation acceptance | Test acceptance |
| --- | --- | --- | --- | --- |
|1 ownership|Requested policy owns command options|Change producer and consumer together|No report-key configuration reads|Poisoned/missing JSON cannot select options|
|2 lifetime|Snapshot predates collection-budget mutation|Preserve admitted replay context ownership|Borrow typed immutable Limits; keep writer lock|Existing actual CLI and storage tests|
|3 simplicity|One private context hides rendering|No generic renderer/schema framework|Public policy input is typed; private annotation is report-only|Literal arrays independently specify wire arguments|
|4 compatibility|Keep argument order, formatting and raw-token policy|Migrate direct Rust callers explicitly|Always render requested memory and case policy|Semantic/raw, best-effort/hard, nested/root scenarios|
|5 verification|Targeted diagnostic scope only|Finish local/native/PR evidence, not a plan-only result|No exclusions or new state strings|Generated policy values, native union and fresh review|

Initial quick diagnostics: 6/8 (7.5/10). Failed: implementation isolation (policy
crosses JSON schema boundaries) and discoverable boundaries (the origin of replay
limits requires reading both caller and renderer). The change must address both;
other six rows pass in this narrow presentation boundary. Final score requires
implementation and review evidence, not this proposed design alone.

## Author review and independent findings

The five design and planning passes above selected the original immutable typed
snapshot, rejected JSON reparsing and generic rendering frameworks, and retained
the actual CLI contract. The implementation passes checked all call sites,
budget-capture order, admitted-context lifetime, option order and raw/semantic
minimization eligibility. The five test passes checked independent literal JSON
oracles, missing/malformed/conflicting display objects, generated numeric limits,
existing native scenarios, and the exact test-inclusive measurement gate.

Compiler-red observed the old bool API rejecting &Limits; implementation then
passed the report suite. Full Rust tests passed 381 cases, strict Clippy passed
on host/Linux GNU/Windows GNU, rustdoc and 28 documentation tests passed.
Independent read-only review found no Critical or Important defects and scored
all eight scoped diagnostics (10/10). Its one Minor finding was real: the literal
"missing" case inserted a null field. An added field-presence assertion failed
against that fixture; complete display objects now exercise true absence, malformed
and conflicting values independently without test-body control flow. The final
27-case report suite passed after that correction; native acceptance remains required.

The reviewer declined native coverage, whole-repository architecture, historical
evidence, engineering-time percentages and pending source hashes. These are
verification/scope limits rather than unresolved behavior decisions: native results
will belong to the current PR check revision, source hashes are refreshed below,
and no whole-repository score or measured time-percentage claim is made.
