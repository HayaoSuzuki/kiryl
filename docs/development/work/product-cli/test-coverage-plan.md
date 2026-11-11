# Test cases and coverage implementation plan

## Design and constraints

One test invocation exercises one scenario or one generated property. Independent
scenarios become named rstest/pytest parameters or separate functions. Test bodies
have no conditional/loop control flow; infrastructure fixtures may supervise one
scenario, and sequence properties use an independent reference model. Rust
parameterized boundaries evolve into proptest invariants without deleting concrete
regressions. Python retains its existing Hypothesis equivalent.

Coverage uses cargo-llvm-cov, instrumenting both Rust tests and the CLI exercised
by Python subprocess acceptance. Its default tests-directory exclusion must be
disabled. Production and test coverage are reported separately and together, with
zero-execution functions visible. Platform-specific code needs native measurement,
not blanket exclusion based on the developer's OS. Line/region/function metrics
must be named; 100% is the target rather than a rounded value. Only unreachable
or externally uninducible paths may receive narrowly scoped inline exclusions
with reasons; ordinary untested behavior must be tested.

## Work

- Refactor Rust integration and verification cases; add generated properties.
- Refactor Rust unit cases and model correspondence; preserve all former assertions.
- Refactor Python cases and infrastructure fixtures without changing product behavior.
- Establish measured baseline, then close uncovered reachable paths and test-code gaps.
- Add repeatable measurement and CI evidence; review exclusions and remaining gaps.
- Run all strict checks and native CI, update existing PR.

## Design self reviews

1. Cases remain independently addressable in runner output.
2. Properties generalize behavior rather than replay a hardcoded table.
3. Moving scenario iteration into helpers is not accepted as parameterization.
4. Coverage includes test source and subprocess CLI execution.
5. Exclusions require a specific impossibility reason, never a desired score.

## Plan self reviews

1. Source ownership split avoids concurrent edits.
2. Dependencies match hoimin/taloon rstest0.28 with default features disabled.
3. Product tests wait until source edits finish because artifact identity hashes source.
4. Native OS coverage is separate from local macOS evidence.
5. Fresh review, test execution and PR/CI are completion requirements.

## Additional Python requirements

The maintainer additionally requires strict Ruff/ty, pytest-cov, pytest-randomly
and exact100% Python coverage. Ruff already selected ALL; ty now also disables
respect for type-ignore comments and enables strict equality/generic narrowing.
The pinned product environment includes pytest-randomly5.1.0 and pytest-cov7.1.0.
Randomization exposed target-order assumptions and monitoring-tool ownership in
fixtures; those were corrected rather than disabling randomization globally.
Product adapter modules reached100% statements/branches locally. Native-only
integration test bodies remain in scope and require native union evidence.

## Implementation self reviews

1. Restored canonical artifact paths so rejection tests reach their intended guards.
2. Unique owned temporary roots avoid collisions after splitting test functions.
3. Instrumented CLI subprocesses add coverage absent from ordinary Rust unit runs.
4. Coverage scripts retain zero-hit functions and reject mixed/missing native evidence.
5. Coverage exposed impossible command states; PreparedOperation now pairs admitted
   context with reproduction kind. ReplayContext stores validated UTF-8 once, and
   JSON Value persistence no longer pretends its formatting can fail.

## Test self reviews

1. Tables have independent runner case names, including Lean-generated row identities.
2. Property tests cover arbitrary streams, scheduler charges, report severity and
   reproduction identities, retaining concrete boundary regressions.
3. Protocol error fixtures exercise malformed and incomplete worker handshakes.
4. Generated scripts/target fixtures are distinct from actual test-function control flow.
5. Local percentages cannot substitute for unexecuted Windows/Linux test bodies;
   exact native aggregation, test-source coverage and fresh source revision are required.

## First combined measured revision

Fresh local collection passes all Rust/Python tests and strict checks. Raw Rust
coverage is3057/3200lines(95.53125%),288/311functions; this remains below the target.
Python has2588/2649statements and354/354branches, with only nativeLinux/WindowsCLI
test bodies missing locally. These are development snapshots with source hashes,
not accepted100% results. Native collection and the exact aggregate gate are the
next evidence step. Two independent tool-review findings were fixed: omitted
source files now fail inventory validation, and source-byte changes are detected
independently of HEAD. Tooling unit tests themselves reach100% statements/branches.
