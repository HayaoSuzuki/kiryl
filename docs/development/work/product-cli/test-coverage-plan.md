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

## Autospec refinement and third native collection

All Python monkeypatch fixtures/objects and unrestricted Mock/MagicMock constructors
have been removed from product and experiment code. Callable doubles use
create_autospec with real signatures or documented foreign-platform interfaces;
scoped patch contexts restore replacement names. Production measurement retains
the real measured factory through ExitStack. Five review passes per phase in the
three testcase review records cover interface fidelity, restoration, one-case
granularity, assertions and strict checks.

The fresh local collection after this refinement passes303Rusttests and259Python
cases with6native skips. RawLLVM totals are3267/3334lines(97.99040191961608%) and
322/325functions; Python has2787/2867statements and374/374branches. The80missing
Python statements belong to native CLI/hard-resource tests. This is not a100%
acceptance claim. Native2 atda7d403 passed Linux3.15 and WindowsRust, exposing
collection-fixture startup/stderr ordering and cross-platform Python fixture issues.
The fixtures now obey startup attachment and write diagnostics before output flood;
bootstrap owns a typed resource-module double and source failure uses an absolute
missing path. Native coverage now runs one hard-resource scenario per parameter
to exercise actual Linux/Windows containment inside instrumentation.

## Native3 evidence and coverage closure refinement

Native run43127630794 at0da1d25 completes the autospec regressions on all six
platform/interpreter cells. The PythonJSON union executes all2867statements and
374branches, including native test implementations. This is development evidence,
not acceptance: Windows instrumentation exposes a0.7second collection-budget
assumption in the short-budget test; Linux3.14 successfully collects coverage but
its launcher sees EBUSY during immediate controller removal. A separate Linux3.14
hard control report has0attempts and only345ms remaining after collection under
its8second budget. The case now allows10seconds (target blocks60seconds), the hard
driver allows30seconds, and owned-controller removal retries for a bounded5seconds
with process/membership diagnostics on failure. No unknown process is killed.

Additional Rust cases cover pending-original collisions, malformed candidate
transport, failed initial reproduction and unfinished active shrink evidence.
Five Linux-only ignored cases exercise actual owned-kernel control permission
failures; the collector includes ignored cases under explicit delegation. Runtime
verification of those cases remains pending. Their restoration guard changes only
the exact post-attachment cgroup recorded by its controlled worker.

Five self-review passes per phase were performed for this refinement:

| Pass | Design | Plan | Implementation | Tests |
| --- | --- | --- | --- | --- |
|1 ownership|Parent retains Search; sessions borrow|Keep corpus behavior before removing optional state|Option<&mutSearch> cannot remove the parent's engine|Existing persisted-corpus cases retained|
|2 evidence|Resource evidence owns label precedence|Preserve diagnostic fields and admission order|Private admission combines OOM override and allowed labels|Eight independent label/evidence cases retain payloads|
|3 failure|Native errors remain transport/storage errors|Inject owned permission failure after start acknowledgement|No generic backend mock or production fault flag|Four replay-phase errors and sticky cleanup tested natively next|
|4 boundedness|Cleanup retains sole process ownership|Use finite traffic and explicit budgets|Controller cleanup waits without killing; Windows wait uses deadline|Backpressure fixture asserts full-channel infrastructure; live owned job tests expired deadline|
|5 scope|No schema or command change|Compare named-revision native reports before acceptance|Same positive-OOM and inactive-worker infrastructure policy reused|324local Rust cases pass; cross-platform Clippy and native matrix remain separate evidence|

The Windows deadline testcase was observed failing compilation before the private
wait boundary existed, then cross-target strictClippy passed. The eight admission
cases likewise failed before implementation and then passed. Native timings and
OS errors are not replaced by unit doubles. No inline or file-level coverage
exclusions were added. Defensive test-helper paths remain visible and will be
closed using the next native report.

## Native4 findings and final counter collection

Atd230a68 both delegatedLinux product cells and all four hard-resource gates pass.
All five real Linux permission-fault cases execute successfully. Windows3.14 also
passes its product cell; Windows3.15's ordinary suite exposes another2second
startup-budget assumption (native abort never executes and returncode is0).
Behavioral CLI proof campaigns and their positive replays now allow10seconds;
case-timeout and campaign-exhaustion scenarios keep their specific explicit limits.

The native4 Unix union leaves four runtime propagation/poll lines and nine
restoration-helper lines. A raw SIGKILL replay permission case and an active-case
periodic permission case cover distinct transport/infrastructure policies. Ten
filesystem cases exercise missing/unreadable markers, outside ownership, corrupt
permissions, restoration/write failures and bounded directory removal. The helper
captures its parent once, never mutates the ambient environment and treats absence
of its owned directory as successful cleanup. Exact helper code passes all ten
cases in a temporary macOS Unix harness; native Linux remains the acceptance test.

Windows counters reveal a measurement discrepancy: CLI-only resource paths remain
zero despite successful instrumented acceptance, while unit counters are present.
Rust's [upstream coverage regression](https://fuchsia.googlesource.com/third_party/rust/+/f58b2548dc702ebc64a1b6e63f0eb1f99dc7851b/tests/run-coverage/while_early_ret.coverage)
records that WindowsMSVCprocess::exit bypasses saved profiling counters. The
[standard-library guidance](https://doc.rust-lang.org/std/process/fn.exit.html)
recommends returning a Termination value, but its generic exit-handler description
does not establish profiler behavior on this build. Main now returns ExitCode
through normal runtime shutdown, preserving0/1/2/130; recovery of counters must be
verified natively before declaring the hypothesis confirmed.

Private Windows command construction owns missing-SystemRoot admission, trusted
program arguments; the common probe owner configures streams and blocking stdin.
Private port association owns its ABI record
and API operation. Four owned-input/owned-handle tests first failed compilation
without those boundaries, then cross-target strictClippy passed. The live-job
deadline test now holds the job through the full500ms budget to exercise polling
and failure without relying on kernel termination delays.

The scoped design review initially passed six of eight diagnostics (7.5/10),
identifying mixed helper comments and implicit periodic/EOF error contracts.
Separate comments now explain admission precedence/payload preservation and active
case classification/error handling. The reviewer rechecked both rows and passed
all eight (10/10 scoped, not whole-repository perfection or coverage acceptance).
Five review perspectives for each phase above were repeated for scope, ownership,
error specificity, bounded lifecycle and measurement identity. No coverage
exclusion, mock backend or product fault flag was introduced.
