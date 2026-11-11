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
