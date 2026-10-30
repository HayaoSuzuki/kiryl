# Coverage-guided Python fuzzing feasibility implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce reproducible evidence that architecture A can execute the
approved test forms, preserve and minimize findings, and guide exploration on
the required runtime matrix, before adopting an engine or building the product.

**Architecture:** A disposable Rust controller retains candidates and controls
separate Python workers. A Python pytest adapter owns fixture lifetimes,
Hypothesis compatibility, branch feedback, and semantic replay/shrinking.
Each task adds an independently testable experiment to this vertical slice.

**Tech Stack:** Repository Rust 1.99.1 / edition 2024; CPython 3.14 and 3.15 GIL
builds; experiment-only Hypothesis 6.170.13 and pytest 9.2.0; `sys.monitoring`;
small Rust byte mutator; native OS process-control probes. Hypothesis 6.170.13
is an intentional initial pin matching the API investigation, not a latest-version
claim. Resolve transitive dependencies into an experiment-specific lockfile.

**Spec:** [Approved design](design.md). Read it alongside this plan.

## Global constraints

- Rust is the primary implementation language; Elastic-2.0 remains the project license.
- macOS, Linux, Windows; CPython 3.14 and 3.15 with the GIL enabled.
- Synchronous pytest/Hypothesis tests, fixtures, parametrization, and stateful testing are required; do not silently omit a failed category.
- Reuse `@given`, strategies, assertions, and explicit preconditions. No new input-definition language.
- One worker by default. Multiple workers run different tests; never fuzz one test concurrently in several workers in the first version.
- Equal exploration-time allocation across selected tests; adaptive allocation is deferred.
- No backward compatibility, migrations, or old-runtime preservation for stored exploration data.
- CLI and machine-readable JSON reports. No GUI or dashboard, including in later scope.
- kiryl needs no account or outbound network connection. Dependency downloads during development are separate from runtime operation.
- Input preservation covers worker failure and orderly interruption, not sudden controller death or machine power loss.
- Publishing, deployment, production engine adoption, and release automation remain deferred.
- Use only independently authored, distributable synthetic examples. Do not record proprietary sources, paths, identifiers, or investigation findings.
- Every reported result records revision, dirty-source hashes when applicable, OS, Python/package versions, command, seed, limits, and outcome. An unrun cell is unverified.

## Review focus

1. Parameter variants and methods sharing one function must not share bound fixture values or misroute saved failures (Tasks 1-2).
2. An example crossing a slice/campaign boundary must not create a false target timeout or abandon ordinary teardown unnecessarily (Task 4).
3. Large/non-UTF8/native stdout must not corrupt control messages or block the worker after log retention fills (Task 4).
4. Shrinking, a new environment, or a failed write must not overwrite the original or silently reinterpret an incompatible replay record (Tasks 2 and 4).
5. Baseline comparisons must not inherit discovered cases or database entries from the coverage-guided run (Task 6).

---

## Scope and stopping rule

### Execution method and additional verification requirements

The maintainer selected inline execution on 2026-10-31: one implementer runs the
tasks in this session, with a fresh whole-branch reviewer at the end. The design
and plan were committed first as `fed3bca` before implementation work.

Every task must record at least five distinct self-review passes: (1) approved
contract and scope, (2) boundary/error behavior, (3) state/side effects and
concurrency, (4) property/fuzz coverage and test sensitivity, and (5) module
boundaries, source confidentiality, and evidence accuracy. Record findings,
repairs, and exact verification results; five repetitions of one test command
do not constitute five reviews. Review records belong in the feasibility results
when substantive evidence exists, with temporary task notes in the plan ledger.

Actively use a small pinned Lean model for pure state/budget/artifact invariants
introduced by the Rust controller; export model expectations and check same-premise
implementation correspondence separately. Keep mathematical claims restricted to
the model. Bound Lean runs externally (initially 20 seconds) and use local
heartbeat limits; no unbounded exhaustive search or `sorry` proofs.

Add `proptest` behavioral properties and run `cargo-fuzz` against the Rust
protocol/state boundary when it exists (Tasks 4-6). Keep these dependencies and
fuzz targets experimental. Run finite fuzz campaigns, retain minimized regressions,
and report actual run counts/time/toolchains. Python-stage randomized checks use
Hypothesis. A not-yet-existing Rust boundary is not reported as fuzz-tested.

Use `gh` for all GitHub access. Do not push, merge, publish, or modify repository
settings as part of inline execution.

Python changes use Ruff 0.17.3 (all lint rules enabled, narrow documented
exceptions, formatting checked) and ty 0.0.95 (all diagnostics at error severity,
warnings fatal). Pin them in the experiment lockfile. Run `ruff check`,
`ruff format --check`, and `ty check` for each stage with Python changes; do not
exclude the experiment from these checks or blanket-disable annotation rules.

This is the executable plan for the design's feasibility phase, not a claim that
the complete first-version CLI can already be planned against proven APIs.
Work through Tasks 1-6 in dependency order. A failed integration gate produces a
reproducible failing probe and a limitation report; do not continue by replacing
required stateful/fixture/platform support with a simpler subset. Routine adapter
fixes and different mechanisms within the approved architecture are permitted.
A required product-scope change returns to design review.

Run short, targeted tests while developing. Execute benchmarks only after the
behavioral probes pass. Never use a timing benchmark as the only correctness test.

## File ownership

All experimental code lives under `experiments/python-fuzzing/`; nothing is
promoted into `src/` by this plan. Use a standalone Cargo package with its own
`[workspace]`, `Cargo.lock`, and the repository toolchain/lint policy. Use a
separate non-published Python project and `uv.lock` in the same directory so
neither development dependency graph adopts an engine accidentally.

| Files | Responsibility |
| --- | --- |
| `experiments/python-fuzzing/{pyproject.toml,uv.lock}` | Exact Python experiment environment; no runtime network requirement |
| `experiments/python-fuzzing/{adapter.py,hypothesis_bridge.py}` | Pytest session/invocation binding and version-specific Hypothesis execution/replay/shrink |
| `experiments/python-fuzzing/monitor.py` | Per-case application branch feedback and monitoring coexistence |
| `experiments/python-fuzzing/{worker.py,wire.py}` | Bootstrap, bounded control messages, and dispatch; not corpus mutation |
| `experiments/python-fuzzing/{Cargo.toml,Cargo.lock,src/main.rs,src/controller.rs}` | Probe CLI and Rust-side worker/candidate/budget ownership |
| `experiments/python-fuzzing/src/{search.rs,artifacts.rs}` | Experimental mutation/novelty selection and immutable finding persistence |
| `experiments/python-fuzzing/src/resource/{mod.rs,linux.rs,windows.rs,portable.rs}` | Capability and process-tree enforcement probes |
| `experiments/python-fuzzing/tests/test_*.py` | Integration/behavioral tests; Rust unit tests stay beside their owning code |
| `experiments/python-fuzzing/specimens/` | Small authored targets, fixtures, state machines, and failure modes |
| `experiments/python-fuzzing/{benchmark.py,evidence.py}` | Isolated comparisons and machine-readable observation records |
| `.github/workflows/fuzzing-feasibility.yml` | Manual six-cell experiment matrix, not a supported-product declaration |
| `docs/development/work/coverage-guided-python-fuzzing/feasibility-results.md` | Durable results once real runs exist; do not create a passing template |

The following interfaces are experiment contracts. The bridge can change its
private implementation during feasibility work; update callers together and
record consequential design changes. Do not introduce a generic engine registry.

## Task 1: Execute ordinary Hypothesis tests inside pytest lifecycles

**Files:** Create the Python environment, `adapter.py`, `hypothesis_bridge.py`,
`specimens/test_scopes.py`, and `tests/test_adapter.py` from the ownership map.

**Interfaces:**
- `Invocation(nodeid: str, buffers: tuple[bytes, ...])` is one finite pytest invocation.
- `CaseResult` contains `nodeid: str`, `kind: str` (`pass`, `reject`, `failure`, `infrastructure`), `canonical: bytes | None`, `fingerprint: str | None`, `replay_token: bytes | None`, and `rendered: str | None`.
- `SessionEvidence` contains `selected: list[str]`, `cases: list[CaseResult]`, and `fixture_events: list[tuple[str, str]]`.
- `CaseDriver` is a protocol with `next_case(nodeid: str) -> bytes | None` and `observe(result: CaseResult) -> None`. `None` ends the current finite invocation. It exposes no Hypothesis objects.
- `run_session(pytest_args: list[str], invocations: list[Invocation], driver: CaseDriver | None = None) -> SessionEvidence` collects once and executes the scheduled finite invocations in one worker session. Without a driver it consumes each invocation's fixed buffers; with one it requests and reports cases incrementally, allowing Rust feedback between cases.
- `bind_target(item: pytest.Item)` returns an adapter-private bound target. All fixture/self/parameter binding occurs before constructing/caching its fuzz callable.

- [ ] **Step 1: Add meaningful failing lifecycle tests and the exact environment.** Pin the two Python packages above; resolve a separate lockfile. Give the experiment its own pytest configuration with `pythonpath = ["."]` and `testpaths = ["tests"]`. Author a zero-draw `st.just(1)` test with a function fixture and two parameter variants, plus module/session fixture counters. For two invocations of one variant and one of the other, assert:

  ```python
  assert evidence.selected == ["test_scopes.py::test_bound[a]", "test_scopes.py::test_bound[b]"]
  assert [case.kind for case in evidence.cases] == ["pass", "pass", "pass"]
  assert evidence.fixture_events.count(("function", "setup")) == 3
  assert evidence.fixture_events.count(("function", "teardown")) == 3
  assert evidence.fixture_events.count(("session", "setup")) == 1
  assert evidence.fixture_events.count(("session", "teardown")) == 1
  ```

  Add class-method binding, marker/`-k` selection, expected exception handling,
  explicit `assume(False)` rejection, setup failure, and no-supported-target
  cases. Assert parameter values never leak between variants and setup failures
  are infrastructure outcomes without an attributed generated input.
- [ ] **Step 2: Run `uv run --project experiments/python-fuzzing --frozen pytest experiments/python-fuzzing/tests/test_adapter.py -q`.** Confirm failure because the adapter is absent, not because the specimens or environment are invalid.
- [ ] **Step 3: Implement the interfaces in `adapter.py` and `hypothesis_bridge.py`.** Let pytest own runtest setup/call/teardown and parameter expansion. Disable Hypothesis `deadline` only for kiryl-controlled execution. A finite invocation returns normally before its function fixtures are torn down. Contain any exact-version internal dependency in the bridge. Probe repeated invocations explicitly; do not fake them by starting a new pytest session for every slice.
- [ ] **Step 4: Run the same command and confirm all lifecycle cases pass.** Record the actual hooks/internal calls used and whether module/session transitions match pytest. If repeated invocation cannot preserve the approved scope semantics, stop with this evidence.
- [ ] **Step 5: Commit the self-contained environment, adapter, specimens, and tests** with message `experiment: bind Hypothesis examples inside pytest lifecycles`.

## Task 2: Prove stateful execution, database replay, and semantic shrinking

**Files:** Modify `hypothesis_bridge.py` and `adapter.py`; create
`specimens/test_findings.py`, `specimens/test_stateful.py`, and `tests/test_replay.py`.

**Interfaces:**
- Consume `Invocation`, `CaseResult`, and `run_session` from Task 1.
- Add `verification: str` (`pending`, `confirmed`, `unstable`) to `CaseResult` for fresh-context verification; do not replace its original failure category with this status.
- `replay(nodeid: str, token: bytes, pytest_args: list[str]) -> CaseResult` enters a fresh worker context and executes one opaque replay record.
- `minimize(nodeid: str, token: bytes, budget_seconds: float, before_case: Callable[[bytes], None]) -> ShrinkEvidence` reports `original: bytes`, `best: bytes`, `fingerprint: str`, `complete: bool`, and `rendered: str | None`. `before_case` publishes a reproducible token before every candidate execution and returns only after controller acknowledgment.
- Bridge-owned replay tokens have an explicit format/version tag. Raw fuzz buffers and serialized Hypothesis choices are separate representations; Rust never interprets the latter.

- [ ] **Step 1: Add failing tests for ordinary assertion and state-machine invariant failures.** Use small synthetic integer/list and create/update/delete models with intentional defects. Verify both a known triggering input (bridge correctness) and subsequent semantic simplification. Assert:

  ```python
  assert shrunk.original == original
  assert replay(nodeid, shrunk.best, args).fingerprint == observed.fingerprint
  assert shrunk.rendered is not None
  assert published_tokens  # Every shrink candidate reaches before_case first.
  ```

  Also assert the minimized ordinary value/operation sequence is strictly simpler
  on the authored specimen's explicit metric; shorter opaque bytes alone do not
  satisfy this test. Run ordinary pytest in a separate process with the same
  configured Hypothesis database and assert it rediscovers the same assertion
  without using kiryl. Verify method/parameter database identity is not changed
  by wrappers. With `database=None`, assert explicit replay still works and the
  evidence reports automatic database replay as unavailable.
- [ ] **Step 2: Run `uv run --project experiments/python-fuzzing --frozen pytest experiments/python-fuzzing/tests/test_replay.py -q` and confirm the missing bridge/shrink capability causes failure.**
- [ ] **Step 3: Implement stateful extraction and semantic replay/shrink in the isolated bridge.** Pin the verified Hypothesis version. Use its shrinker rather than reimplementing strategies in Rust. Bound shrinking by remaining budget and require the same observable failure fingerprint. Unsupported versions/tokens fail explicitly. Do not write raw fuzz buffers as Hypothesis database entries.
- [ ] **Step 4: Run the replay suite and Task 1's suite.** Include zero shrink budget (`complete == False`, original retained), a competing failure, an incompatible token, and a failure depending on prior external state (`unstable`, without fabricated standalone reproduction). Confirm ordinary pytest's own deadline/settings differences are reported.
- [ ] **Step 5: Commit** with message `experiment: verify stateful replay and Hypothesis shrinking`. If the adapter cannot provide a pre-execution replay token during shrinking, record the limitation and stop before claiming native-crash recovery for that phase.

## Task 3: Attribute per-case branch feedback without disturbing other tools

**Files:** Create `monitor.py`, `specimens/branches.py`, and `tests/test_monitor.py`;
modify `adapter.py` to attach features to each case observation.

**Interfaces:**
- Add `features: tuple[str, ...]` to `CaseResult`; consumers treat identifiers as opaque.
- `BranchMonitor(roots: tuple[Path, ...])` provides `start_case() -> None`, `finish_case() -> tuple[str, ...]`, and `close() -> None`.
- IDs include code/interpreter context and are stable between workers in the same recorded environment, not across arbitrary code/Python updates.

- [ ] **Step 1: Add failing tests with a two-branch synthetic function.** Run left, right, then left again and assert:

  ```python
  assert left and right
  assert set(left) != set(right)
  assert repeated_left == left
  assert all(feature.startswith("specimen:") for feature in left + right)
  ```

  Test that generator/pytest/Hypothesis branches do not create novelty, another
  occupied monitoring ID is untouched, unavailable IDs produce a clear
  infrastructure result, and a second synthetic monitoring client continues to
  receive its callbacks after kiryl resets its own per-case counters.
- [ ] **Step 2: Run `uv run --project experiments/python-fuzzing --frozen pytest experiments/python-fuzzing/tests/test_monitor.py -q` and verify failure.**
- [ ] **Step 3: Implement `BranchMonitor` with `BRANCH_LEFT`/`BRANCH_RIGHT`.** Select an unused tool ID, apply project-code filtering, clear only kiryl's own per-case observations, and release only its own events/tool. Do not use global event reactivation to reset individual cases.
- [ ] **Step 4: Run the monitor suite and the existing experiment suites on each available target interpreter.** Record a separate pytest-cov coexistence probe once a compatible experiment-only version is resolved and locked; distinguish tested configurations from unsupported ones. Compare the direct adapter with monitoring enabled/disabled, without imposing an invented performance threshold.
- [ ] **Step 5: Commit** with message `experiment: collect isolated Python branch feedback`.

## Task 4: Drive the adapter from Rust and recover findings after worker death

**Files:** Create the standalone Rust manifest/lockfile, `src/{main,controller,artifacts}.rs`,
`worker.py`, `wire.py`, `specimens/test_process_failures.py`, and `tests/test_controller.py`.

**Interfaces:**
- Probe CLI: `fuzz-probe run --config CONFIG --out DIRECTORY`; configuration supplies interpreter, pytest selection, scheduled inputs, limits, and seed. These are experiment commands, not the product CLI.
- `Controller::run(config: ProbeConfig) -> Result<ProbeReport, ProbeError>` retains every active candidate until completion, including test and case IDs.
- `ProbeConfig` owns `python: PathBuf`, `pytest_args: Vec<String>`, `invocations: Vec<Invocation>`, `limits: Limits`, `seed: u64`, and `search: bool` (default false). Its Rust `Invocation` owns `test_id: String` and `inputs: Vec<Vec<u8>>`; the wire maps these to Task 1's node IDs and byte buffers. With `search == true`, the controller supplies streaming cases from Task 6 instead of the fixed inputs.
- `Limits` owns optional campaign duration, per-case duration, slice duration, per-tree memory bytes, retained-output bytes, and `allow_best_effort: bool`. Initial probe defaults follow the design (unlimited campaign, 30-second case, 60-second slice, 1 GiB memory, 1 MiB logs, best-effort disabled); tests override them with short bounded values.
- `Observation` owns `kind: String`, `canonical: Option<Vec<u8>>`, `fingerprint: Option<String>`, `replay_token: Option<Vec<u8>>`, `rendered: Option<String>`, `features: Vec<String>`, and `verification: String`, corresponding to the Python case result. Controller correlation supplies test/case IDs.
- `ArtifactStore::record_original(test_id: &str, input: &[u8], observation: &Observation) -> Result<FindingId, StoreError>` creates an immutable original; derived replay records have separate IDs/lineage.
- `ProbeReport` serializes test/case outcomes, original/derived references, verification status, timing, bounded log bytes, and actual enforcement capabilities. JSON protocol/version validation is owned by `wire.py` and the Rust controller transport area.
- `FindingId` is a string; `StoreError` is `std::io::Error`. `ProbeError` distinguishes configuration, transport, and storage errors. Probe wire/artifact schemas start at integer version `1` and reject other versions without deleting files.

- [ ] **Step 1: Add failing controller integration tests.** Build a small `probe_run(config)` test fixture that invokes the real Rust probe and returns its JSON report; no mocked worker in these tests. Author targets for `os.abort()`, an infinite loop, a normal assertion, and a stdout/stderr flood containing invalid UTF-8. Assert:

  ```python
  assert crash.original_bytes == supplied_buffer
  assert crash.kind == "native_crash"
  assert flooded.retained_log_bytes <= 1024
  assert flooded.log_truncated is True
  assert flooded.control_messages_valid is True
  assert budget_cancel.kind == "campaign_cancelled"  # Not an example timeout.
  ```

  Test independent worker restart, normal slice overrun followed by teardown,
  per-case timeout, zero remaining verification budget, and orderly interruption.
  A malformed control frame is infrastructure failure. An incompatible artifact
  version or failed derived write must leave the original untouched.
- [ ] **Step 2: Run `cargo test --manifest-path experiments/python-fuzzing/Cargo.toml` and `uv run --project experiments/python-fuzzing --frozen pytest experiments/python-fuzzing/tests/test_controller.py -q`.** Confirm missing implementation failures; bound every intentionally hanging probe from its parent test process.
- [ ] **Step 3: Implement the probe interfaces and bounded transport.** A practical experiment bootstrap reserves the original stdout stream for control, redirects target FD-level stdout to the log stream before importing specimens, and drains logs concurrently; validate this on Windows as well as Unix. Worker bootstrap waits for a controller session-start acknowledgment before importing targets, allowing resource attachment in Task 5 before target execution. Keep framing/IDs/version bounds independent of arbitrary target output. Parent retains raw candidates before dispatch and atomically records originals before verification/shrink. Add the shrink-candidate acknowledgment from Task 2. No per-input disk synchronization or power-loss guarantee is required.
- [ ] **Step 4: Run both commands and Tasks 1-3's integration tests.** Exercise full, partial, and malformed frames; Ctrl-C/portable cancellation; an input finishing after its slice but before its case deadline; global expiry during setup/execution/shrink; and independent-test continuation after a finding. Use fake monotonic time for allocation-unit tests and real subprocesses for termination tests. Initial probe exit codes: `0` no finding, `1` finding/candidate, `2` infrastructure error, `130` interruption; record all categories in JSON even for mixed outcomes.
- [ ] **Step 5: Commit** with message `experiment: supervise Python fuzz cases from Rust`. Keep experimental Rust dependencies in the standalone manifest/lockfile and apply the repository's format, Clippy, and license/source checks to that package as well.

## Task 5: Verify actual memory/output/descendant capabilities on each OS

**Files:** Create `src/resource/{mod,linux,windows,portable}.rs` and
`tests/test_resource_probes.py`; extend the controller and synthetic process specimens.

**Interfaces:**
- `Capabilities` reports `tree_memory_hard: bool`, `tree_cleanup: bool`, `backend: String`, and an explanatory `reason: Option<String>`.
- `resource::detect() -> Capabilities` performs a real availability/permission probe, not merely an OS-name check.
- `resource::attach(worker: &mut Child, limits: &Limits, allow_best_effort: bool) -> Result<Guard, ResourceError>` either establishes the disclosed guarantees or fails before target execution.
- `Guard::terminate_tree(&mut self) -> Result<(), ResourceError>` is the sole descendant-cleanup owner. OOM observations require backend evidence, not an arbitrary kill status.

- [ ] **Step 1: Add failing resource integration tests.** Spawn a worker which starts a child allocator, then separately a descendant that outlives its root. Assert:

  ```python
  assert denied.started_target is False
  assert denied.kind == "infrastructure"
  assert fallback.capabilities.tree_memory_hard is False
  assert unrelated_kill.kind != "oom"
  assert original_survives_limit_termination is True
  ```

  In a hard-capable environment, assert combined parent/child memory triggers
  an evidenced limit event and cleanup removes descendants. On macOS or an
  unavailable hard backend, assert default refusal and explicit best-effort
  opt-in with truthful capability fields. Test unsupported permissions without
  pretending a skipped hard-limit test proves enforcement.
- [ ] **Step 2: Run `uv run --project experiments/python-fuzzing --frozen pytest experiments/python-fuzzing/tests/test_resource_probes.py -q` and confirm the missing capability/guard behavior fails.**
- [ ] **Step 3: Implement minimal capability probes and guards.** Read the pinned hoimin implementations linked by the design; preserve attribution for reused material. Use Linux cgroup v2 and Windows Job Objects only after proving actual permission/assignment. Portable fallback exposes its weaker guarantees. Apply limits before the target can spawn descendants; test that assignment has no uncontrolled launch window. Do not introduce a full portable resource framework.
- [ ] **Step 4: Run the resource suite on each available OS plus Task 4's tests.** Record separate results for hard-capable and fallback modes. If the runner cannot provide a delegated cgroup, that hard-enforcement observation remains unverified until run in a capable environment. Confirm two worker trees get independent allowances and combined retained stdout/stderr remains bounded under termination.
- [ ] **Step 5: Commit** with message `experiment: verify platform resource-control capabilities`.

## Task 6: Measure guided search and produce the six-cell decision evidence

**Files:** Create `src/search.rs`, `benchmark.py`, `evidence.py`,
`specimens/test_discovery.py`, `tests/test_search_evidence.py`, and
`.github/workflows/fuzzing-feasibility.yml`. Create `feasibility-results.md` in
this document's directory only after real observations exist. Update the owning
OKF decision with actual results and its evidence source hash/revision.

**Interfaces:**
- `Search::new(seed: u64) -> Search`, `Search::next_case(&mut self) -> Vec<u8>`, and `Search::observe(&mut self, input: &[u8], observation: &Observation) -> ()` implement deterministic seeded byte mutation and retention based on opaque branch novelty.
- `benchmark.run(output: Path, seed: int, budget_seconds: float) -> Path` runs each comparison with disjoint fresh corpus/database directories and writes JSON evidence.
- `evidence.validate(path: Path) -> list[str]` rejects missing provenance, overlapping comparison storage, fabricated platform completion, or missing required probe categories.

- [ ] **Step 1: Add failing novelty and evidence tests.** Use deterministic observations to check that a new branch retains its candidate, a rejected case is counted but does not masquerade as useful execution, and the same seed/feedback stream reproduces the same search sequence. Assert:

  ```python
  assert guided_database != baseline_database
  assert guided_database.parent != baseline_database.parent
  assert initial_baseline_entries == []
  assert evidence.validate(complete_record) == []
  assert evidence.validate(record_claiming_an_unrun_platform)
  ```

  Add a controlled feedback ablation: identical seed/input proposals with
  branch observations disabled must change retention relative to observations
  enabled. This proves feedback use, not discovery superiority.
- [ ] **Step 2: Run the standalone Cargo tests and `uv run --project experiments/python-fuzzing --frozen pytest experiments/python-fuzzing/tests/test_search_evidence.py -q`; confirm meaningful failure.**
- [ ] **Step 3: Implement a minimal corpus-guided mutator and comparison runner.** Connect `search == true` in the controller to the Task 1 `CaseDriver` wire implementation so every completed case feeds back before generating the next one. Start with bounded zero/random buffers, elementary byte insertion/deletion/replacement, and novelty-based retention; canonicalize only valid passing buffers using the bridge. Compare guided exploration, feedback-disabled mutation, and ordinary Hypothesis on an independently authored branching function and a state-machine invariant defect. Known failure inputs used in replay tests must not be initial discovery seeds. Use seeds `0,1,2,3,4` with 20 seconds per test per mode, counting setup in wall time. Record attempts, useful/rejected cases, observed application branches, discoveries, time to first discovery, replay/shrink cost, and adapter/IPC timing. Do not claim a statistical advantage from this small sample.
- [ ] **Step 4: Add a `workflow_dispatch`-only matrix over `ubuntu-latest`, `macos-latest`, `windows-latest` and Python `3.14`, `3.15`, with a 20-minute timeout and read-only permissions.** Reuse and recheck the SHA-pinned checkout, setup-python, setup-uv, and artifact-upload actions from the existing workflows. Install only the experiment lockfiles, build the Rust probe, run the behavioral suites, then bounded comparisons; upload evidence even on a probe failure. Keep this experimental workflow separate from production support declarations. Use the repository's actionlint/zizmor commands from the developer entry point.
- [ ] **Step 5: Run all experiment suites and bounded comparisons on available environments, validate every evidence file, and write the results.** Record native crashes with exact supplied raw input, semantic shrink on ordinary/stateful failures, settings/DB caveats, platform enforcement limits, and unrun cells. A passing gate requires actual branch-driven discovery and replayable planted failures, not merely direct execution of a known counterexample. No requirement to outperform ordinary Hypothesis. If fixed sample budgets find nothing, report failure/inconclusive discovery rather than changing the benchmark to include the answer.
- [ ] **Step 6: Review the evidence against every gate in the design, then commit** with message `experiment: record guided-search feasibility evidence`. Document failed gates and concrete next experiments. Do not promote code/dependencies into the product or declare Windows/Python support without the native results.

## Verification and handoff

During execution, apply the repository's [Rust checks](../../index.md) and
[documentation checks](../../documentation.md) to relevant changes; run standalone
Cargo checks with `--manifest-path experiments/python-fuzzing/Cargo.toml` and
its own locked dependencies. Keep the root Rust/development checks passing.
Experiment Python tests use their own environment and explicit test directory;
the root suite remains the documentation-tooling suite.

The executor creates the report only from observed commands. CI configuration
is not proof that a remote workflow ran. Do not request a merge, publish a
package, or modify repository settings as part of these experiments.

## Follow-on work after the evidence gate

| Product area | What the next plan can decide from evidence |
| --- | --- |
| Python adapter | Adopt proven fixture/stateful/shrink mechanisms and exact compatibility policy |
| Rust search/executor/artifacts | Select the engine, transport, process backends, replay format, and bounded persistence using measured behavior |
| Campaign and CLI | Implement unlimited/finite budgets, equal-time multi-test scheduling, different-test multiworkers, first-finding continuation, standalone replay/minimize, and full exit/report semantics |
| Product acceptance and docs | Run the full six-cell acceptance suite; document implemented commands/prerequisites; update current OKF contracts alongside code |

These are dependent follow-on plans, not omitted promises of this feasibility
deliverable. Full multiworker campaign scheduling, final CLI packaging, stable
report ergonomics, and user installation guidance are not implemented by these
experiments. Draft those tasks only after the bridge and engine evidence permits
concrete interfaces. Preserve the approved scope if a different mechanism is needed.

## Plan self-review

The plan was checked against the design: test/fixture/stateful integration is
Tasks 1-2; coverage is Task 3; input ownership, time, failure/output handling,
and replay artifacts are Task 4; OS enforcement is Task 5; measured engine and
platform evidence are Task 6. Product campaign/CLI completion is explicitly the
dependent follow-on scope above. Every Review Focus condition has a test in its
owning task. Interface names and ownership are consistent across tasks. No
experiment has been executed by writing or reviewing this plan.
