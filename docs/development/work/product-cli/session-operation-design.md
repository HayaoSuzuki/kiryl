# Typed session operations

The executor session boundary currently accepts a string mode independently of
an optional semantic token. Callers must understand which combinations are valid,
and replay silently defaults a missing token to empty bytes. This contradicts the
already typed public reproduction boundary; it is an internal design gap, not a
new public feature or an observed user-facing failure.

Use a private borrowed SessionOperation enum: Explore, Replay(token),
Minimize(token), RawReplay. A semantic token may be empty, but cannot be absent.
The enum owns start-frame serialization, including raw replay's `run` wire mode.
The transport dispatch matches the operation and frame together, retaining the
existing correlation, expiry, retention and cleanup policies. Public CLI, JSON,
artifact lineage and execution ordering remain unchanged.

Alternatives: constants leave token/mode combinations independent; a general
request/context framework adds indirection and optional fields. The private enum
is the smallest interface covering the four existing operations. Search ownership
and transport-state extraction are deliberately outside this change.

Initial scoped diagnostic score: 5/8 = 6.25/10. Failed rows: implementation changes
require caller knowledge of wire modes; the session interface lacks its contract;
new readers must inspect the body to know valid combinations. Typed variants,
owned frame construction and interface comments address these respectively.
This is not a whole-repository score.

# Implementation plan

One task: preserve session behavior while replacing independent mode/token inputs.
Read set: src/executor.rs, its existing frame fixtures, the architecture concept,
and the test-quality contract. Write set: executor and the owning documentation.

1. Add literal rstest cases for all four start frames, empty semantic tokens and
   opaque nonempty bytes. Observe the tests fail before introducing the type.
2. Implement the enum/frame constructor; migrate production calls and fixture
   inputs. Dispatch replay directly with its required borrowed token, without a
   fallback or unreachable arm. Keep public reproduction modes typed until output.
3. Run all Rust/Python suites and strict checks. Collect native coverage including
   tests; require exactly 100% lines/functions and statements/branches across six
   cells. Update the existing PR and inspect all24 checks.

Review focus: absent versus empty token; raw run wire mode versus semantic replay;
zero-byte preservation; replay-case correlation; minimize-only frame admission;
original locking/retention; cleanup and native counter collection.

# Five self-review passes for each phase

| Pass | Design | Plan | Implementation check | Test check |
| --- | --- | --- | --- | --- |
|1 scope|One private operation concept|Keep public contracts|No CLI/JSON changes|Literal frame oracles|
|2 validity|Token coupled to semantic modes|Remove fallback|Exhaustive enum dispatch|Empty token distinct from absence|
|3 ownership|Borrow opaque bytes|No new token copies at boundary|Clone only retained active input|Arbitrary token property|
|4 depth|Enum owns wire mapping|No generic request framework|No pass-through accessors|Existing protocol negatives retained|
|5 compatibility|Existing supervision unchanged|Native checks required|Review expiry/storage/cleanup diff|Full measured source inventory|

These are author self-reviews, not independent or native acceptance. Final
implementation/test outcomes and diagnostic re-evaluation follow verification.

# Review and local verification

The independent read-only reviewer compared the full changed boundary to `9fe9ff5`,
including adapter protocol and unchanged assertions. Critical: none; Important:
none; Minor: none; no requested behavior declined. All eight diagnostics pass
(10/10 scoped). Replay-case IDs, zero/empty tokens, raw dispatch-once, minimize
admission, pending evidence order, store lifetime and periodic-versus-EOF errors
remain unchanged. No score is assigned to the whole executor or repository.

Implementation self-review repeated the five passes above against the actual diff:
all production callers use typed operations; only frame construction owns adapter
mode/token JSON; retained input is cloned only at case admission; public report
labels stay independent of adapter modes; no protocol/storage/cleanup changes.

Tests first failed with the missing SessionOperation definition, then passed with
the implementation. Full Rust verification passes 331 tests; strict Clippy passes
on the host and Linux/Windows cross targets. Fresh coverage collection passes
259 Python tests with 6 native-only skips and374/374 Python branches locally. Source
freeze was retained throughout collection. Full native union and current PR
checks remain acceptance requirements, not inferred from these local results.

Test self-review checked each literal against the existing worker start contract,
confirmed the generated property preserves opaque bytes, retained every existing
negative frame test, inspected full-suite output, and kept native/test-source
coverage gates mandatory. No exclusion or Python mock change was introduced.

No user-guide update is needed: CLI commands, output and stored schemas are
unchanged. The architecture concept now owns the typed-operation decision.
