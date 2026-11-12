# Rust integration and verification testcase review

Scope: `tests/artifact_store.rs`, `tests/campaign.rs`, `tests/cli_errors.rs`,
`tests/report.rs`, their shared temporary-directory helper, and the test module
in `src/verification.rs`. Review performed by the implementing Codex agent on
2026-11-12 against the working tree based on `7bfea28`. These are self-reviews,
not independent approval or a platform compatibility claim.

## Review 1: testcase design

Design: separate independent artifact operations and verification verdicts into
individual invocations. Multiple assertions remain when they describe one
operation's result and retained evidence. Named rstest cases retain each former
manual-table input. Proptest adds semantic fingerprint/rendering independence,
native exit-detail discrimination, report order/nesting invariance, and
interruption precedence over arbitrary outcome classifications.

Plan: preserve the former assertion obligations before extending reachable error
paths. Implement fixtures that prepare one scenario without running assertions
or iterating through independent cases.

Implementation review: original immutability, lock exclusion and reopening were
three independent scenarios in one function; they are now separate. Structured
record serialization and derivative preservation are separate as well.

Test review: compare the former input tables and assertions to the new named
cases; no original classification or rejected-source scenario was removed.

## Review 2: isolation and cleanup

Design: each test invocation owns a unique temporary directory with PID and an
atomic counter; a guard removes it on normal return or panic. A moved store is
retained under a second owned directory and restored before the writer drops.

Plan: make split cases safe under Rust's concurrent test execution without a
new dependency. Shared setup performs filesystem preparation only.

Implementation review found an incorrect assumption in the first failed-save
fixture: `ArtifactStore::open` does not write a manifest. The fixture was corrected
to move the entire store, preserving its original behavior and failure trigger.
This was a test-fixture error, not an implementation/documentation disagreement.

Test review: the corrected missing-destination case and all independently split
filesystem cases pass concurrently in the integration test run.

## Review 3: compatibility and failure specificity

Design: distinguish saved semantic identity, saved raw identity and automatically
observed native identity. Keep absent-fingerprint equality and saved timeout
exact-detail compatibility explicit.

Plan: name each former verification table case, then generate arbitrary optional
fingerprints and rendered details beyond the fixed examples.

Implementation review: saved identity inputs group the actual observation fields
in a tuple, keeping rstest-generated wrappers below Clippy's argument limit.
Schema rejection now uses a matching target variant; variant rejection uses the
supported schema. Each invocation therefore tests its named admission failure.

Test review: all 31 verification cases pass, including generated Unicode strings
and optional fingerprints. Generated native detail suffixes are nonempty, so the
negative property always changes the recorded exit detail.

## Review 4: reachable coverage extensions

Design: exercise observable storage/report failure outcomes without simulating
unreachable allocator or filesystem internals.

Plan: inspect the existing Rust-only missing-line report and add direct public-API
cases for useful gaps. Treat its recorded percentages as evidence of its earlier
instrumented source snapshot, not the final working tree.

Implementation review: added oversized manifest identity, original record and
derived evidence rejection; corpus/report rename cleanup; replay context identity
access; and campaign raw-finding presentation with effective resource limits.
The raw presentation case verifies omission of unsupported minimization and
retains the neighboring pass observation unchanged.

Test review: the new cases pass. Remaining internal exhaustion and impossible
path-conversion branches were not claimed covered. Whole-product coverage is
measured separately by the parent task with native Python subprocesses.

## Review 5: final structure and checks

Design/plan review: every testcase invocation represents one input scenario or
one generated property instance. There are no `if`, `for`, `while`, `loop`, or
`match` statements in the owned test function bodies. Setup helpers select one
source or prepare one manifest; none runs a table of hidden assertions.

Implementation review: production code is unchanged. RAII cleanup handles
per-case temporary storage, and strict Clippy reports no unused helper methods.
One final literal-format correction (`2_097_152`) resolved the unreadable-literal
lint without changing the scenario.

Verification completed on the final test sources:

- `cargo test --locked --test artifact_store --test campaign --test report --test cli_errors`:
  45 artifact, 5 campaign, 21 report, and 1 CLI-error tests passed.
- `cargo test --locked --lib verification`: 31 verification tests passed.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`:
  passed after the literal-format correction.
- `cargo fmt --all`: completed; the parent task owns the final formatting check.

No user behavior, production contract, or design decision changed in this scoped
refactor, so no OKF contract update is needed for these test edits. This evidence
belongs under the existing product work record, rather than in user guidance.

## Follow-up: Python bridge coverage cases

The additional `python/tests/test_bridge_coverage.py` suite contains 31 passing
single-scenario cases. Its own statements and branches were fully exercised in
the isolated coverage run. Comparing its executed lines/arcs with the earlier
`/private/tmp/python-unit-branch.json` report closes every previously missing
statement and branch in `adapter.py` and `hypothesis_bridge.py`. The parent task
must rerun the combined suite to establish final product-wide percentages.

The cases use actual pytest collection/protocols and real Hypothesis execution.
They cover invalid replay choices, deterministic choice/template replay, opaque
strategy holes, setup skips, streaming retention, version/digest guards, unittest
owned-method restoration and lifecycle failures, missing subprocess targets,
shrinking deadlines, a failure disappearing during semantic shrinking, and a real
pytest plugin claiming the execution protocol without running its target.
Target bodies are isolated specimens; the actual testcase functions contain no
conditional or loop statements. Small monkeypatches change only compatibility
version/digest inputs and the shrink clock.

Five follow-up self-review checks were completed: (1) one scenario per named
invocation; (2) genuine collection, fixture lifetime and subprocess boundaries;
(3) deterministic clock/choice inputs and bounded stream length; (4) preserved
original/replay evidence in error and incomplete-shrink scenarios; (5) strict
static checks and measured coverage gaps. Ruff with the project's `ALL` rules,
Ruff formatting, and strict ty all pass for the new file. The isolated command
`cd python && .venv/bin/python -m pytest tests/test_bridge_coverage.py --cov --cov-report=json:/private/tmp/bridge-coverage.json --cov-fail-under=0 -q`
passed all 31 cases on native macOS CPython 3.14.9.

During this work, a zero-choice constant property that fails and then disappears
during shrinking raised Hypothesis's `RunIsComplete` through the infrastructure
boundary. The variable-choice disappearing-failure regression establishes the
reachable non-interesting-initial-result branch; it does not claim to resolve
that separate zero-choice behavior. Production source remains unchanged.

## Follow-up: sequenced Rust reproduction supervision

Seventeen additional executor test invocations exercise original failure
verification, separately persisted pending shrink evidence, fresh candidate
confirmation/mismatch/skip/incomplete/infrastructure results, tokenless semantic
minimization rejection, malformed candidate tokens, bounded command-queue
backpressure/disconnection, and recovery from poisoned diagnostic storage.

Five review checks covered scenario independence, immutable original lineage,
protocol fidelity, concurrent execution, and verification evidence. Initial
concurrent execution exposed a fixture defect: dumping all worker frames without
reading commands can fill the intentionally bounded supervisor command queue.
The sequenced POSIX-shell worker now consumes the actual length-prefixed command
after each handshake/case/result frame before proceeding. It selects per-launch
frame scripts through a private counter file and requires no Python interpreter.
The final standalone summary is selected from the report's last outcome because
an infrastructure observation may be reported before that summary.

The whole Rust workspace passed with default parallel execution: 162 library,
4 binary, 49 artifact, 5 campaign, 14 CLI-error and 23 report tests. Strict
workspace/all-target/all-feature Clippy passed. The focused instrumented run
passed all 75 executor tests. Its report includes all executor test/helper
lines; remaining uncovered lines are in production supervision and platform/error
paths, and are not claimed covered by this focused run. The parent task owns
the native aggregate and final coverage threshold. No production executor code
was changed by this follow-up.

## Follow-up: autospecced mocks

The Python adapter, bridge, monitor and CLI-helper tests now use scoped
`unittest.mock.patch.object` contexts. Callable substitutes are created with
`create_autospec` and `spec_set=True` before replacing their originals; constant
Hypothesis versions and platform identifiers are patched as values. This replaces
the earlier pytest monkeypatch usage described above. There are no unbounded
`Mock()` or `MagicMock()` constructions in these four files.

Five review checks covered restoration at context exit, callable signatures,
object attribute bounds, one scenario per test, and unchanged coverage. Popen
and Path substitutes use the real standard-library classes, and require only
class-defined methods. Windows-only kernel and ctypes error APIs use narrow
Protocols documenting the relevant Microsoft/CPython signatures, so Unix tests
cannot silently invent unrelated attributes. No arbitrary attribute list or
production mocking interface was added. The existing absolute missing-source
monitor callback specimen remains unchanged.

The behavior follows Python's documented [autospeccing and scoped patching
contracts](https://docs.python.org/3/library/unittest.mock.html#create-autospec).
The foreign-interface declarations reference [Microsoft handle cleanup](https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-closehandle)
and [ctypes error handling](https://docs.python.org/3/library/ctypes.html#ctypes.get_last_error).
Ruff's complete configured rules and strict ty with `--python-platform all` pass
for these four files. The targeted suite passes 99 cases, and each maintained
file retains 100% statement and branch coverage in its measured run. Production
source and behavior are unchanged, so no OKF contract update is needed.
