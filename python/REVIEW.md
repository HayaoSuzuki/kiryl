# Python product adapter review

This record covers the Python portion of Issue #5 only. Sources were the
approved product implementation plan, architecture contract and experimental
adapter modules. `gh issue view 5` failed because network access was unavailable.

## Design review passes

1. Ownership: Rust retains generation, budgets and persistence; Python retains
   pytest/Hypothesis binding, replay and branch feedback. No search engine added.
2. Lifecycle: retained invocation cloning and real pytest setup/teardown rather
   than calling target functions directly. Fixture correspondence tests retained.
3. Dependency boundary: product runtime imports only its own modules and pinned
   installed dependencies. Experimental fixtures copied into product tests.
4. Identity: collection records exact selection and recursive project-relative
   SHA256 source/config snapshot. External services, data and unrecorded packages
   remain reproduction assumptions. Symlinks are excluded explicitly.
5. Information hiding: software-design-philosophy score 8.75/10. Seven diagnostic
   rows pass; independent implementation replacement still requires agreement on
   replay envelopes and wire fields. Reaching 10 requires a separately versioned
   transport contract with compatibility fixtures; this change preserves the
   existing tested protocol rather than claiming that work complete.

## Plan review passes

1. Promote lifecycle/bridge/monitor/wire/bootstrap individually, not the
   experiment's benchmark/search or specimen runtime.
2. Write collection regressions first; observed both fail because collect.py
   did not exist before implementation.
3. Retain behavioral lifecycle, semantic replay/shrink and monitoring tests.
4. Check actual isolated no-site bootstrap with a real subprocess handshake.
5. Run native tests and strict tools; record environment failures independently
   of successful checks. No Windows/Linux support inferred from macOS execution.

## Implementation review passes

1. Found copied monitor subprocess paths assumed the old flat layout; corrected
   cwd to product module directory. Initial suite: 54 pass, two path failures.
2. Collection reserves duplicated stdout before dependency/target import and
   redirects fd 1 to stderr. Raw os.write output is covered.
3. Stateful factory conversion and synchronous inner-test inspection preserve
   stateful and parametrized IDs while excluding async/fixed functions.
4. Runtime admission checks require GIL CPython 3.14/3.15, pytest 9.2.0 and
   Hypothesis 6.170.13 before target execution in worker dispatch.
5. SHA256 snapshot traversal sorts paths, excludes generated environments and
   symlinks, and records selected root and arguments. Reads can fail loudly;
   snapshotting is not an atomic filesystem snapshot.

## Test review passes

1. Collection test asserts supported count, stateful ID and unsupported exclusion;
   import-error test requires nonzero status and parseable JSON.
2. Added source-edit test proving hashes change and .venv is excluded.
3. Added -I -S worker ready test proving bootstrap does not need site packages
   before the controller attaches resources.
4. Retained behavioral suites cover fixtures, skips, semantic replay/shrink,
   ordinary Hypothesis compatibility, monitoring ownership and subprocess
   feature stability; these are not implementation-mirroring assertions.
5. Native macOS arm64 CPython 3.14.9: 58 passed. Ruff check/format and ty check
   passed with the pinned experiment environment used only as a test interpreter.
   Repeated in product python/.venv after creating a CPython 3.14 environment
   and copying the pinned installed packages locally: 58 passed in 6.92s.
   Product runtime has no experiment path/import. CPython 3.15.2 was installed,
   but offline uv sync could not obtain cached pygments; using 3.14 native
   Hypothesis binaries in 3.15 failed before collection. Therefore no 3.15 suite
   success is claimed. Linux/Windows native execution remains unverified here.

## Invocation

Collection: `<python> python/kiryl_adapter/collect.py [pytest selection args]`.
Stdout is one JSON object containing version, nodeids, root, sources, selection
and environment. Collection errors use nonzero exit with an error field.

Worker: `<python> -I -S -c 'import sys; sys.path.insert(0, ABS_ADAPTER_DIR);
import worker; worker.main()'`. Ready frame precedes site.main and contains
platform, Python major/minor and implementation. All subsequent protocol and
lifecycle behavior retains the promoted bounded wire/bootstrap contract.

## Native CLI integration follow-up: captured stdin

The parent agent's native CLI check found that pytest capture replaced sys.stdin
while Driver.next_case attempted to read the protocol. A new product regression
first reproduced this failure. Driver now retains the original text wrapper
before pytest starts (retaining just its binary buffer would allow wrapper
finalization to close the pipe). All Driver operations and replay/minimization
callbacks read the retained stream. Bootstrap's initial receive happens before
pytest and is unchanged.

Five follow-up checks reviewed ownership, wrapper lifetime, all receive call
sites, callback paths and regression assertions. Final native macOS CPython
3.14 suite: 59 passed in 7.55s; Ruff check, format check and ty passed. This fixes
the Python regression; the parent owns verification of the full native CLI.

## FD capture correction and native CLI acceptance

The first stdin follow-up test changed only sys.stdin and missed pytest's fd-0
redirection. The parent rerun exposed EOFError despite that test passing. The
regression now runs a subprocess which replaces fd 0 with /dev/null and then
replaces sys.stdin. It failed with truncated control frame before the correction.
Driver now reserves os.dup(stdin.fileno()) and wraps that duplicate unbuffered;
all post-setup receives use this independent descriptor. The original wrapper
retention fix was insufficient and has been superseded.

The actual product CLI was run from /private/tmp/kiryl-product-fixture against
test_sample.py with the product interpreter, --best-effort --duration 3 and fresh
store/report paths. It exited 1 (findings), without the previous infrastructure
error. Report: /private/tmp/kiryl-python-fd-fix-report.json. Five follow-up review
passes checked FD ownership, buffer independence, replay/minimize call sites,
regression strength, and native CLI outcome.

## Product CLI acceptance regression review

`python/tests/test_cli.py` invokes the built root target/debug/kiryl and current
product interpreter. Binary absence skips explicitly; CI must cargo build before
running this suite. Six end-to-end tests cover persistent corpus/restart, planted
failure with fresh replay/minimization and immutable original, changed-source
rejection, -k selection with parameter fixtures and stateful tests, exact active
input retained on case timeout, exhausted-budget later target visibility, and
macOS hard-limit rejection before target import (combined where appropriate).

Five design/test review passes:

1. Behavioral assertions read persisted corpus/original/report results, not
   private controller methods. Passing finite searches allow exit 1 only because
   a campaign boundary can preserve an unresolved active candidate.
2. The source mismatch test changes valid source with a comment and checks the
   original bytes remain unchanged after rejection. It cannot pass merely from
   a syntax/import error.
3. Selection test exposed discovery running before pytest -k deselection. The
   collector now reads session.items at collection_finish; direct collection
   and full CLI tests cover the correction.
4. Timeout test checks case_timeout and exact initial [0] * 512 active input.
   Short-budget test checks the later selected target remains unattempted in the
   report. Slices use 0.5 seconds; 0.1 seconds could finish before startup admitted
   a case, making a corpus assertion meaningless.
5. Hard-limit test checks an import-created marker remains absent on macOS after
   infrastructure rejection. The executor's newly guarded collection startup is
   exercised. Linux/Windows hard-limit behavior is not inferred from this test.

Final verification after cargo build --locked --quiet: native macOS arm64
CPython 3.14.9, 65 tests passed in 17.70 seconds. Ruff check, format check (16
files) and ty check all passed. No CPython 3.15 or other-OS acceptance claim.

## OS-facing CLI regression additions

Added actual product CLI tests for FD-level non-UTF-8 stdout/stderr plus child
output under a combined 1024-byte retention budget, native abort preserving the
exact initial candidate with fresh raw replay, and untimed POSIX SIGINT yielding
exit 130 plus interrupted report. Conditional tests exercise Windows direct
GetStdHandle/WriteFile output and Linux worker-descendant cleanup after timeout.
The latter recognizes Linux zombie state as exited, and always attempts cleanup
of its spawned PID on test failure. Windows console-interrupt remains a native acceptance gap; no macOS assertion claims a
hard tree cleanup capability which the portable backend does not provide.
A separate Windows Job Object cleanup test now checks descendant status using
OpenProcess/GetExitCodeProcess, rejects permission-denial ambiguity and cleans
up on failure. It is conditionally skipped locally and requires native CI.

Five review passes checked (1) real process execution versus mocked outcomes,
(2) bounded combined log assertions including binary/descendant output,
(3) immutable exact crash input and fresh-worker replay semantics, (4) interrupt
synchronization on a target-created marker rather than a fixed startup sleep,
and (5) platform-specific skips and failure cleanup. The raw replay test exposed
an executor report-classification bug (native abort called failure), corrected
by the parent agent. These additions do not establish other-OS success locally.

Latest local full suite before the final Windows-only conditional addition: 68
passed, 2 skipped in 20.41s. The Windows-only addition was checked with ty and
collected locally: two Windows tests skipped as expected.

## CI slice-counter assertion correction

Native macOS CI exposed that the last-slice report can have useful=0 after
previous slices executed useful cases. The corpus restart and output tests now
assert target totals.useful, the controller's aggregate campaign counter,
without increasing durations. Rebuilt current binary and ran those two focused
CLI tests: 2 passed, 10 deselected in 6.31 seconds. Ruff check, format check and
ty passed. This corrects the observation scope rather than weakening the
requirement that useful work actually occurred.

## CI campaign-output assertion correction

macOS 3.15 and Windows 3.14 CI exposed the analogous last-slice log issue: an
empty final slice does not erase earlier output. FD/descendant and Win32 handle
tests now assert campaign retained_output.tail, observed and truncated, while
retaining the separate last-slice combined-cap assertion. Durations are unchanged.
Rebuilt latest binary: focused output tests yielded 1 passed, 1 Windows skip,
10 deselected in 2.43 seconds on macOS 3.14. Ruff check, format check and ty passed.
Native Windows execution of the revised assertion is delegated to CI.

## Shared runtime admission before collection

A new subprocess regression monkeypatches Hypothesis's version and supplies a
target that creates an import marker. Before the fix collection returned zero
and executed the target; the regression failed. runtime.py now owns interpreter,
GIL, pytest and Hypothesis admission. Collection validates before pytest.main;
worker validates before importing adapter/Hypothesis internal bridge. The bridge
uses the same Hypothesis version constant rather than duplicating a pin.
Unsupported collection runtime returns structured JSON error with exit 2.

Five review passes checked admission order, deferred dependency imports after
resource/stream reservation, shared pin ownership, JSON rejection behavior and
marker strength. Selection's hook class was moved out of main to keep its
collection responsibility explicit without adding a pass-through API.
Verification: collection/bootstrap/replay targeted suites 22 passed in 4.86s;
Ruff check, format and ty passed. Native matrix rerun is owned by the parent.

## Linux best-effort cleanup capability distinction

Ubuntu CI showed that the ordinary-descendant cleanup regression incorrectly
required capabilities.tree_cleanup=true. The portable backend conservatively
disclaims hard cleanup of detached descendants even when its process group
terminates an ordinary inherited child. Removed that capability assertion from
the best-effort test; retained the actual /proc exit/zombie assertion and failure
cleanup. The Windows Job Object test keeps its separate native-capability claim.
This change makes no stronger portable guarantee. Ruff, format (17 files) and ty
passed locally; focused Linux test collected and skipped on macOS (1 skipped,
11 deselected). Actual Linux execution remains delegated to native CI.

## Final error-report CLI regressions

Added three actual CLI regressions against the parent's rebuilt final binary:
fixture setup RuntimeError retains its sentinel in infrastructure outcome.detail;
collection import failure replaces a stale successful JSON report with status
infrastructure and nonempty infrastructure_errors; an existing writer.lock
produces a target infrastructure outcome/detail without changing the lock.
These were added after the Rust fixes and are not claimed as red-green evidence.

Five review passes checked (1) setup diagnostics originate from pytest reports,
(2) stale-report replacement is asserted by absent stale key and changed status,
(3) lock failure is isolated from source/runtime failures, (4) lock contents remain
unchanged, and (5) infrastructure exit 2 is checked independently of report shape.
Focused native macOS CPython 3.14 run: 3 passed, 12 deselected in 0.80 seconds.
Ruff check, formatting and ty passed. No additional feature work was introduced.

## Windows console interruption acceptance gate

Added a Windows-only untimed CLI test using a real CREATE_NEW_PROCESS_GROUP
and CTRL_BREAK_EVENT. It checks GetConsoleCP and allocates an actual console
with AllocConsole when the CI process lacks one; allocation failure fails the
test rather than simulating success or skipping on Windows. It waits for a
marker written inside the running target, then requires exit 130 and an
interrupted report. Finally kills a still-running parent and frees only a
console allocated by this test. The earlier missing Windows-console test gap
now has a regression; native execution remains unverified locally.

Five checks reviewed native event delivery, process group isolation, console
ownership, marker-based timing and failure cleanup. Ruff check, formatting and
ty passed. Local macOS focused collection: 1 expected platform skip, 15
deselected in 0.08 seconds. Native Windows CI is the required remaining gate.
No runtime source was changed for this test.

## Cross-platform static typing correction

Windows native ty identified signal.SIGKILL in the Linux cleanup test because
pytest's skip decorator does not narrow sys.platform for static analysis. Added
an explicit Linux guard inside that test before native operations, preserving
normal native execution and all behavioral assertions. No typing rules were
disabled. ty check passed for --python-platform win32, linux and darwin with the
product interpreter; Ruff and formatting checks passed. This validates platform
symbol resolution, not native execution of other-platform tests.
