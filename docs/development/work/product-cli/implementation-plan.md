# Single-worker product CLI implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Track steps with checkboxes.

**Goal:** Implement Issue #5: persistent single-worker Python fuzz campaigns, fresh replay and semantic minimization.

**Architecture:** Rust owns campaign state, search, execution limits and storage. A separate Python adapter owns pytest, Hypothesis and Python branch feedback. Promote mechanisms individually from the experiment with product-specific tests; do not depend on the experiment at runtime.

**Tech Stack:** Rust 1.99.1, edition 2024; GIL-enabled CPython 3.14/3.15; pytest 9.2.0; Hypothesis 6.170.13. Review dependencies and licenses before adoption.

**Spec:** [Approved design](../coverage-guided-python-fuzzing/design.md), GitHub Issue #5.

## Global constraints

- Single worker; synchronous given, fixtures, parametrization and stateful tests.
- No account, telemetry, package publication, deployment or engine plugin framework.
- Original findings survive failed verification, minimization and worker termination.
- Missing hard tree-memory enforcement rejects execution unless best effort is explicitly selected.
- Platform support is established by native product CLI execution, never by configured CI jobs.
- Every phase receives five separate self-review passes with findings and resolutions recorded. Do not pre-mark reviews or tests complete.

## Proposed product contracts

These concrete choices specialize the approved design and require review before implementation.

`kiryl run [PATH_OR_NODE...]` accepts `--python`, `-k`, `-m`, `--duration`, `--case-timeout`, `--slice`, `--memory-mib`, `--output-bytes`, `--store`, `--seed`, `--best-effort`, and `--report`. Durations are positive decimal seconds; omitted campaign duration means run until interrupted. Defaults: python3, 10-second case timeout, 1-second slice, 1024 MiB memory, 65536 retained output bytes, .kiryl store. No pytest option forwarding that can bypass adapter ownership.

`kiryl replay FINDING` and `kiryl minimize FINDING` accept interpreter, duration, resource and report options. The artifact supplies target identity and environment expectations. Each verification uses a fresh worker. Minimize stores a separate candidate and confirms its fingerprint before reporting success.

Exit codes: 0 for completed search without findings or unresolved candidates; 1 for findings, reproduced failures, unstable/unverified candidates; 2 for invocation, environment, persistence or execution infrastructure errors; 130 for user interruption. Reports preserve all applicable statuses rather than compressing them into the exit code.

JSON reports have schema_version 1, command, effective limits, actual resource capabilities, environment identity, per-target counters/status, finding references and infrastructure errors. Unexecuted targets remain visible. Report output is atomic when a path is supplied; diagnostics go to stderr. Counters and log tails stay bounded independently of trial count.

Storage version 1 owns manifests, per-target corpus and immutable originals with separate derivatives. Keys include canonical project-relative node ID and parameter variant. Environment identity records interpreter/platform, adapter and dependency versions, project source hashes and effective selection/configuration. Record precisely which files were hashed; external services, mutable data and unrecorded packages remain reproduction assumptions. Reject incompatible versions or detected mismatches without deleting originals. Bound corpus count and bytes; use coverage novelty for exploration eviction, never for original-finding eviction. Serialize store writers with an exclusive campaign lock.

Campaign state tracks remaining monotonic wall budget across collection/setup/search/verification/minimization. Case timing starts after setup. Slice expiry stops admission and permits an active case to finish within its case/campaign limit. Scheduling debits actual exploration time including cooperative overshoot. Finding persistence precedes verification. Cleanup and final atomic persistence may exceed campaign duration and are reported separately.

## File ownership

- `src/main.rs`: CLI parsing and exit mapping only.
- `src/campaign.rs`: budgets, scheduling and finding state transitions.
- `src/search.rs`: mutation and bounded useful corpus selection.
- `src/executor.rs`, `src/executor/`: wire transport, bounded drain, process trees and capability enforcement.
- `src/artifacts.rs`: versioned identities, atomic persistence and immutable originals.
- `src/report.rs`: stable product report serialization.
- `python/kiryl_adapter/`: pytest session, version-pinned bridge, monitoring and worker entry point.
- `tests/`: product Rust and subprocess integration regressions.
- `formal/`, `fuzz/`: executable Lean expectations and finite fuzz targets.

## Review focus

1. Disk-full/partial writes must preserve existing originals and surface infrastructure errors.
2. Hanging setup and descendant processes must consume campaign budget and be cleaned up.
3. Non-UTF-8, FD and descendant output must never corrupt control frames.
4. Changed source/configuration or target variants must reject reuse without deleting data.
5. Short budgets and overshooting tests must retain unexecuted statuses and fair time accounting.

## Task 1: Product contracts and persistence

- [ ] Update existing OKF architecture/store owners with adoption reasons, alternatives and reconsideration conditions; explicitly record scaffold versus experimental evidence.
- [ ] Add failing Rust tests for immutable original writes, unsupported schemas, variant separation, mismatch rejection, locking and injected write failures.
- [ ] Implement `ArtifactStore::open`, `save_original`, `save_derivative` and bounded corpus load/save in `src/artifacts.rs`; return typed errors, never silently repair incompatible artifacts.
- [ ] Run targeted tests, five implementation reviews and five test reviews; commit this independently reviewable boundary.

## Task 2: Native executor and Python adapter

- [ ] Add product regressions corresponding to experimental lifecycle, replay, protocol and resource tests before promoting code.
- [ ] Implement `Executor` as the sole process/transport owner; expose target discovery and bounded invocation outcomes to campaign code, keeping wire types private.
- [ ] Implement the packaged adapter and reject unsupported Python/Hypothesis versions before target execution; preserve pytest fixture/session lifetimes and stateful semantics.
- [ ] Exercise setup errors, case deadlines, interrupted frames, binary/FD output, child output, tree cleanup and hard-limit rejection. Windows tests include direct Win32 handles and ambiguous termination classification.
- [ ] Run strict Rust checks, Ruff ALL/format and ty all-error checks; record five implementation and five test review passes and commit.

## Task 3: Search, scheduling and CLI

- [ ] Add failing tests for budget precedence, fair exploration-time allocation, overshoot debit, first-finding target stop, remaining-target continuation and unexecuted statuses.
- [ ] Implement bounded `Search` and `Campaign::run` using executor outcomes and artifact methods. Persist active exact input before verification or recovery transitions.
- [ ] Implement CLI contracts above and JSON reports; integration-test discovery, run-until-interrupt, finite campaigns, restart from corpus and failure presentation.
- [ ] Generate Lean executable expectations for state/budget/original invariants and compare the Rust transitions against them. State modeling exclusions explicitly.
- [ ] Add proptest and finite cargo-fuzz targets for wire/store/state boundaries. Record duration, runs, toolchains and regressions.
- [ ] Complete five implementation and five test review passes; commit.

## Task 4: Fresh replay and minimization

- [ ] Add failing subprocess tests for reproducible, unstable, skipped, rejected, budget-exhausted and worker-crash findings.
- [ ] Implement replay and semantic minimization through the adapter and executor. Candidate success requires fresh fingerprint confirmation; retain originals and unfinished candidates.
- [ ] Verify ordinary pytest replay/database behavior and explain settings/database differences in user guidance.
- [ ] Complete strict checks and five implementation/test review passes; commit.

## Task 5: Acceptance and CI

- [ ] Extend existing product CI using hoimin's finite fuzz budget/corpus retention and taloon's separated Rust/Python quality jobs; retain SHA pins, read-only permissions and job timeouts. Do not import publishing workflows.
- [ ] Run product end-to-end scenarios on macOS/Linux/Windows × CPython 3.14/3.15; retain revision/environment evidence. Delegated-cgroup Linux hard enforcement is a separate required execution, not a skip counted as success.
- [ ] Measure long runs: elapsed time, trials, RSS, saved bytes and bounded histories. Compare ordinary Hypothesis with equal seed/time and isolated databases/corpora, including negative results.
- [ ] Update user commands/prerequisites and existing OKF owners with actual evidence and remaining resource gates.
- [ ] Complete five final implementation/test review passes, all repository checks and final review before creating a PR. Do not claim unexecuted platform acceptance.

## Design self-review record

1. Scope: mapped run/replay/minimize, persistence and multi-test fairness to Tasks 1–4; excluded publishing and multiple workers.
2. Boundary/error review: added writer locking, mismatch rejection and explicit infrastructure exit status; no automatic destructive migration.
3. State/side effects: original persistence precedes derivatives; monotonic shared budget and cooperative slice completion are distinct.
4. Test sensitivity: added disk failure, output corruption and unexecuted-target cases; native resource gates remain required.
5. Design complexity: eight-diagnostic provisional score 7.5/10 (six passing). Interface abstraction comments and newcomer comprehension remain unverified until implementation. Require promise-focused API comments and test module boundaries against actual callers before a higher score. No public generic engine abstraction is planned.

## Plan self-review record

1. Coverage: all Issue #5 stages mapped; comparison and native resource evidence explicitly belong to Task 5.
2. Step precision: tasks identify owners and observable failures; detailed method signatures beyond ArtifactStore/Executor/Campaign must be settled in task-local test design before code.
3. Consistency: one executor owns wire/OS details; reports consume typed outcomes; Python internals remain in the bridge.
4. Failure coverage: each of the five review-focus conditions has an owning task and required regression.
5. Proportion/evidence: the plan does not mark future execution as evidence. Remaining planning work is task-local signatures, exact corpus caps and report field enums; settle and review these before implementing their owning tasks.

Implementation and test reviews are pending, not represented by the design/plan passes above.
