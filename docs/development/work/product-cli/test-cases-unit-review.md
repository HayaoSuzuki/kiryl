# Rust unit test case review

Scope: test modules in `src/executor.rs`, `src/campaign.rs`, `src/search.rs`,
`src/wire.rs`, `src/main.rs`, `src/resource/mod.rs`, and `src/resource/linux.rs`.
The remaining resource platform files contain no tests to split.

Five review passes were performed over this scope in the working tree on
2026-11-12. These are distinct checks by the implementing agent, not five
independent reviewers.

1. Case inventory: separated malformed transport frames from incompatible JSON
   versions, exit-code table entries, duration acceptance from rejection,
   discovery success/empty/error, baseline reporting from reproduction modes,
   initial progress from accumulation/error, and pass/failure/reject search
   observations. Each rstest invocation receives one input case.
2. Assertion preservation: checked the before/after module diffs. Truncated-tail
   merging and saturation are distinct cases; campaign failure still compares
   all previously accumulated counters, charge and retained bytes. Lean budget
   and cleanup rows retain their generated expected values, with a separate
   corpus-size check requiring new generated rows to receive parameters.
3. Property review: arbitrary chunk streams compare retained suffixes to an
   unbounded concatenation and verify observed counts and truncation flags.
   Added search feature idempotence, seeded stream determinism and input bounds,
   campaign saturation and replacement of cumulative charge, and concatenated
   wire-frame boundary preservation. Each generated value describes one stream
   or transition sequence; iterator folds do not enumerate independent cases.
   Arbitrary decoding and arbitrary framed reading now have separate tests.
4. Structural review: test bodies contain no explicit `if`, `for`, `while`,
   `loop`, or `match` expressions. The Unix exit polling helper performs setup
   for one child; the resource iterator repeats termination on one guard to
   measure its idempotency. All seven production prefixes before their test
   modules were compared byte-for-byte with HEAD and were identical.
5. Execution review: the full Rust workspace suite passed with all features
   (86 library tests, four main tests and 38 integration tests before the two
   corpus-size guards). Strict all-target Clippy reported only an eight-argument
   rstest in the separately owned verification module; the needless borrow in
   the log-tail fold was corrected. Final results are recorded below.

An initial campaign property tried JSON serialization of unrestricted u128
charge values and encountered serde_json's number range limit. The property
now checks typed state directly, keeping its full u128 domain without changing
production serialization behavior. Its temporary regression artifact was
removed because it recorded the invalid test oracle.

No user guide or OKF contract update is needed: executable behavior and design
contracts are unchanged. This record documents the test organization and new
verification only. Native Linux-only test invocations remain outside this
macOS run.

Final scoped observation: 88 library tests and four main tests passed after
adding corpus-size guards. The concurrent workspace integration rewrite then
failed `failed_save_preserves_original` at `tests/artifact_store.rs:80` with
ENOENT, outside this module scope. Strict Clippy still reported only
`src/verification.rs:218` (`too_many_arguments`). These are handoff observations,
not a claim that the final combined tree passes all checks.

Coverage closure added explicit restore, mutation-size, feature-budget,
corpus-capacity, duplicate/oversized candidate and wire I/O/size boundaries.
The final isolated library run passed 110 tests. Its LLVM JSON report includes
`src/search.rs` at 144/144 lines and 21/21 functions, and `src/wire.rs` at 92/92
lines and 14/14 functions. No source exclusions were added. The report was
written to `/private/tmp/kiryl-unit-coverage.json`; the combined CLI measurement
has separate ownership. Strict workspace all-target Clippy passed on this tree.
All changes remain within the test modules; production source is unchanged.

Executor coverage closure uses a test-only Unix interpreter fixture: a shell
writes preencoded bounded control frames, then remains alive until supervised
cleanup. It exercises real subprocess pipes and process-group attachment without
changing worker production code or requiring a Python fixture dependency.
Individual parameter cases cover mismatched test/case identities, malformed and
unknown observations, result-without-case, active-case completion, skip
correlation, unknown/error frames, and changed environments. Additional cases
cover interrupted minimization evidence, replay admission, closed command input,
invalid executor settings, missing interpreters, mandatory resource rejection,
and collection timeout/interruption/malformed/oversized output with stderr.

The final isolated run passed 142 library tests (58 executor cases), and strict
all-target Clippy passed. Comparison of instrumented segment starts with the
recorded combined baseline shows 114 previously zero executor source lines now
exercised by unit tests; this is not a combined line-coverage percentage.
`/private/tmp/kiryl-executor-unit-coverage.json` contains the isolated report.
The production prefix of executor remains unchanged. Native OOM evidence and
resource-monitor failures require native backend execution or explicit fault
injection; macOS best-effort execution cannot establish those paths. Other
remaining gaps include reproduction/verification alternatives and abnormal
cleanup/interleaving failures and are not declared unreachable.

Coverage-tool review added 57 isolated tests for collection and aggregation.
Both scripts and their test file reach 100 percent statements and branches in
`/private/tmp/kiryl-tools-final-coverage.json`; strict Ruff and ty checks pass.
Tests mock subprocess orchestration, so exercising the collector cannot recurse
into another pytest run. Test bodies contain no conditional, loop, match or
comprehension expressions.

The collector retains lexical virtual-environment interpreter paths, chooses the
native executable suffix, propagates instrumentation to actual CLI subprocesses,
and invalidates only its previous generated success metadata. The aggregate
requires all six platform/Python cells and consistent full revision IDs, keeps
zero-hit functions distinct even on the same source line, rejects negative or
undefined function counts, and preserves missing statements and branch arcs.

Independent review identified two additional false-success risks: omitting an
entire source file from every report, and editing uncommitted source while HEAD
stays unchanged. The collector now records normalized-LF SHA-256 input manifests
and working-tree state, compares source bytes before and after testing, and the
aggregate compares all manifests with its current checkout. It also checks the
native union against every scoped Rust/Python/test/tool source. The Rust library
entry point is exempt only while its contents consist entirely of comments and
module declarations; a test proves executable content removes that exception.
All product adapter and test files remain in coverage scope. The two new tools
are included explicitly in the reporting scope; no coverage exclusions were added.
