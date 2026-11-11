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

## Native feedback and second measurement

The first native run at0cfe0b2 exposed a test-fixture race: prewritten worker frames
could overrun the bounded command queue without consuming acknowledgments. All
nonempty fake workers now consume actual length-framed commands. Windows strict
Clippy also rejected the larger PreparedOperation layout; its admitted context is
now boxed without changing lock lifetime. Linux3.15 produced usable coverage, but
its ordinary job lacked cgroup delegation, leaving native attachment unmeasured.
A shared dedicated-subtree launcher now serves both hard gates and coverage while
running product commands as the ordinary runner. No shared ancestor is modified.

Further tests cover filesystem-backed cgroup parsing/control failures, ownedWindows
job/port events, malformed observations, storageIDexhaustion, report persistence,
root locks, rejected raw minimization and presentation-time corruption. Guard and
notification policies were extracted into testable private boundaries without
backend traits, additional runtime flags, or coverage exclusions.

The second fresh local collection passes298Rusttests and258Pythoncases with4native
skips. RawLLVM totals are3189/3283lines(97.13676515382272%) and317/321functions;
Python has2713/2774statements and374/374branches, with61Windows/LinuxCLItestlines
unexecuted locally. This is still not an accepted100% result. FiftyLinuxpure/filesystem
cases also passed against the exact source through a temporaryUnixharness, and both
Linux/Windowscross-target strictClippy passed; nativeWindows execution is pending.

Function gating now matches LLVM source-function semantics: generic instantiations
share source coordinates while distinct same-line definitions retain their columns.
It validates everyLCOVdefinition against JSONmetadata before merging. The measured
localfunction count matched LLVM's own summary exactly; instantiation/region rates
remain separate and are not claimed as100%. Independentreview rechecked source
inventory, snapshot checks, quoting and delegation ownership without finding a
Critical/Important issue. Cleanup may emit an additional diagnostic if setup fails
before the controller exists; this is fail-closed and is not a coverage exclusion.
