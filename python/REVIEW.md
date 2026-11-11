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
