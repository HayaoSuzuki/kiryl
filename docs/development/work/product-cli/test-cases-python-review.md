# Python test case review

Scope: Python adapter and documentation tests in the working tree based on
`7bfea28`. These changes reorganize tests and add error-path observations;
production behavior and the owning contracts remain unchanged. No OKF concept
update is needed for this test-only work. Coverage infrastructure and its
acceptance decision are maintained separately by the parent task.

## Design reviews

1. Identified independent method binding, expected-exception and rejection
   scenarios; each now receives its own pytest parameter case.
2. Retained sequence scenarios when the behavior under test is state across
   inputs: fixture invocation schedules, coverage resets and corpus restart.
3. Kept branchy authored target strings intact: these are specimen programs,
   rather than control flow in the tests executing the observations.
4. Classified process polling, resource cleanup and independent monitoring
   callbacks as infrastructure. Their helpers now have explicit boundary tests.
5. Kept native acceptance tests visible with platform skip markers. Mocked native
   API checks provide deterministic branch coverage without claiming native
   Windows or Linux acceptance on macOS.

## Plan reviews

1. Audited actual test function ASTs, including nested callbacks and expression
   comprehensions, rather than searching source strings for keywords.
2. Selected parameter tables for independent cases and retained Hypothesis for
   arbitrary invocation schedules and coverage-reset sequences.
3. Planned projections through `operator` functions and explicit comparisons,
   keeping strict Ruff rules enabled instead of suppressing comprehension rules.
4. Selected direct invocation of the monitoring callback retrieved through the
   public monitoring API: Python tracing does not trace native callback dispatch.
5. Separated discovery selection from discovery classification, and replay
   identity mismatch from replay/minimization immutability checks.

## Implementation reviews

1. Actual `test_*` functions contain no `if`, `for`, `while`, conditional
   expressions or comprehensions. Infrastructure helpers may contain lifecycle
   control flow, with independently exercised branches.
2. The generated boolean sequence property compares every measured observation
   against reference positive/negative observations. It does not conceal a loop
   over unrelated test scenarios in a helper.
3. Monitoring initialization failure restores slot ownership; callback tests
   cover inactive cases, identity caching, excluded environments and source errors.
4. Worker tests exercise real frame encoding and decoding while substituting
   descriptor acquisition and native APIs. Malformed acknowledgments are separate
   parameter cases and cannot silently count as passing operations.
5. CLI binary selection accepts `KIRYL_BINARY` for an instrumented executable.
   The short-budget specimen disables target reordering explicitly because its
   assertion requires the blocking target to precede the unexecuted target; the
   surrounding test suite remains randomized.

## Test reviews

1. Strict `ruff check python/tests`, formatting, and strict `ty check --project
   python` pass without adding lint or type ignores to bypass the new policy.
2. Documentation tooling tests passed: 28 cases before the final unchanged
   diagnostic-projection formatting check.
3. A configured non-CLI coverage run passed 148 cases and measured 100% statement
   and branch coverage for collect, monitor, runtime, wire and worker. Its JSON
   evidence was `/private/tmp/python-unit-final.json` during this session; this is
   temporary evidence, not a committed acceptance artifact.
4. Newly added CLI lifecycle helper tests passed all 26 cases. They exercise
   readiness deadlines, cleanup of reaped PIDs, native cleanup errors and console
   allocation ownership, without invoking a real product campaign.
5. The final full macOS run passed 192 cases with four native-platform skips in
   27.86 seconds. All seven product modules and every exercised test/helper file
   measured 100% statements and branches. The aggregate was 97.53% because 60
   lines in skipped native Windows/Linux CLI scenarios require native report
   aggregation. No coverage exclusions were added. Temporary JSON evidence:
   `/private/tmp/python-full-second.json`.

The final run used a separate coverage data file to avoid concurrent report
writers. Earlier invalid runs overlapped an independently edited bridge specimen
and exposed a randomized target-order assumption. A later randomized run exposed
an ambiguous monitoring-slot lookup in the direct-callback fixture; the fixture
now finds the slot allocated by its own monitor and closes it even if setup
fails. These failures were corrected before the passing full run. Native report
aggregation remains the parent task's final acceptance gate; the macOS run does
not establish native Windows/Linux acceptance.

## Independent structure and portability review

A separate review of the other contributors' current test structure inspected
135 Rust functions carrying `#[test]` or `#[rstest::rstest]`. A Rust lexer removed
comments and string literals before inspecting their balanced function bodies;
no `if`, `for`, `while`, `loop`, or `match` tokens remained in those bodies.
Sampled parameter helpers configure the selected scenario, while executor loops
supervise a worker lifecycle. They do not iterate unrelated scenarios behind a
single test invocation. Python AST inspection likewise found no conditional or
loop constructs in actual tests, including nested callbacks and comprehensions.

Strict ty checks passed for all six analysis combinations: Linux, macOS and
Windows with Python 3.14 and 3.15. This review corrected two portability issues:
the Linux process-tree test now asserts its native prerequisite so the type
checker can resolve `SIGKILL`, and the simulated Windows cleanup exception uses
an integer `winerror`, consistent with native Windows `OSError` typing. The 26
cleanup-helper cases still pass; Ruff and formatting remain clean.

Coverage-tool review identified two limitations for the parent task to resolve
or document. The aggregator checks nonempty metric sets but has no expected
source inventory, so an otherwise valid six-cell matrix omitting an entire
module could appear complete. Also, collection compares Git HEAD before and
after execution; this detects commits but cannot detect changed dirty working
tree bytes at the same HEAD. A clean CI checkout mitigates the latter; local
reports must retain their working-tree qualification. Neither finding required
production behavior changes in this review.

## Autospec revision: worker, bootstrap and runtime tests

This revision follows the maintainer's replacement of pytest monkeypatch with
standard-library patch contexts and `create_autospec`. The
[official create_autospec documentation](https://docs.python.org/ja/3/library/unittest.mock.html#unittest.mock.create_autospec)
was read before editing: callable specs constrain available APIs and supported
signatures, while `spec_set=True` rejects attributes absent from the spec.

Design reviews:

1. Keep real file descriptors and frame serialization in Driver tests; replace
   only the boundaries requiring deterministic faults.
2. Specify the Driver double from a real initialized Driver instance so its
   dynamically initialized `input` and `case_id` attributes remain bounded.
3. Use native loader originals as callable specs; represent absent foreign APIs
   with named protocols documenting their arguments and ctypes descriptors.
4. Treat interpreter versions, platform strings and module dictionaries as
   values replaced through scoped patches rather than callable mocks.
5. Keep the isolated resource module: bootstrap tests must neither require Unix
   modules on Windows nor change the runner's actual core-dump limits.

Plan reviews:

1. Capture real callables before installing their replacements.
2. Restore module, descriptor and callable patches through `ExitStack` fixture
   teardown, including exceptional exits.
3. Replace handwritten replay/shrink callables with autospecced originals whose
   side effects retain the authored acknowledgment behavior.
4. Split GIL capability admission from the table of noncallable interpreter
   values, preserving one scenario per invocation.
5. Measure modified test sources as well as worker/runtime code; directly check
   the resource-spec fallback rather than leaving a host-dependent hole.

Implementation reviews:

1. The three owned files contain no pytest monkeypatch fixture or operations.
2. No unrestricted `Mock()` or `MagicMock()` constructors remain; `Mock` is used
   only for type annotations around an autospecced real Driver instance.
3. Callable replacements use `create_autospec(..., spec_set=True)`; every
   `patch.object` supplies an explicit replacement rather than creating a default
   unrestricted mock.
4. Native protocol declarations cover `prctl`, `SetErrorMode`, CRT abort behavior
   and resource-limit setup. Opaque foreign calls remain bounded by those
   documented APIs instead of an arbitrary attribute tree.
5. Original argument and frame assertions remain. The prctl call uses exact
   `call_args` equality plus a single-call assertion, preserving the five-argument
   check alongside autospec's invocation signature validation.

Test reviews:

1. All 34 targeted cases pass with randomized seed 17.
2. Worker and runtime product modules measure exactly 100% statements and
   branches in the targeted run.
3. All three modified test files likewise measure exactly 100%; temporary JSON
   evidence is `/private/tmp/kiryl-autospec-worker-final.json`.
4. Ruff and formatting pass for the owned files; strict ty passes all six Linux,
   macOS and Windows × Python 3.14 and 3.15 analysis combinations.
5. AST inspection finds no branches, loops, conditional expressions or
   comprehensions in the owned test functions. Full-suite/native aggregation
   remains the parent task's gate after the other test files finish conversion.


## Stateful runtime interception review

The remaining production and experimental `instrument_ordinary` implementations
used pytest's patch manager to install the real `measured_factory`. They now use
`ExitStack` with standard-library `patch.object(..., new=measured_factory)` and
return `ExitStack.close` as the restoration callback. This changes the scoped
replacement mechanism, with no intended change to generation, shrinking,
database identity or restoration on exceptions.

The replacement remains a real function: it calls the captured original
Hypothesis factory, instruments the returned real handle, and returns its real
wrapped property. An autospecced mock would turn runtime instrumentation into a
test double and could alter execution or Hypothesis database identity. Autospec
belongs at the tests' fault-injection boundaries, not in this production wrapper.
The patch import is local to stateful measurement to avoid importing mocking
infrastructure during ordinary product startup.

The experiment's version mutation and PYTHONPATH setup now use scoped value
patches; its Linux bootstrap faults use the same bounded native API autospec
pattern as the product tests. Separate experiment lockfiles, dependency versions
and tooling configuration were left unchanged.

Validation: 45 product replay/bridge cases passed, including ordinary and
stateful database reuse. Eighteen affected experimental/bootstrap/replay cases
passed in the experimental virtual environment. Experimental Ruff, formatting
and all six native-platform/Python-version type analyses passed; the changed
product bridge passes Ruff and formatting. A repository-wide Python search found
no remaining pytest MonkeyPatch or monkeypatch operations. The parent task owns
full-suite/native verification after concurrent test edits finish.

Source snapshots read for this review (`working_tree: modified`):

- Product bridge SHA-256: `220afdb79a150c404ef81beda0099820b16a5f2628b426b0e7d5c9d85bb90a4e`.
- Experimental bridge SHA-256: `8fb21823f685db6435d669fcd9d88670750b479bb9e99123f3c0c85a01b555b7`.
