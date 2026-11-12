# Product integration review

This record describes working-tree implementation, not native matrix acceptance.

## Design passes

1. Scope checked against Issue #5: run, stored-corpus restart, replay, semantic
   minimize and single-worker multi-target selection now have product paths.
2. Failure boundaries reviewed: collection must acquire containment before
   target imports; unavailable hard memory rejects without executing project code.
3. State ownership reviewed: campaign root lock spans all slices; storage retains
   exact active originals before verification; derivatives never replace them.
4. Scheduling reviewed: charge actual exploration interval including cooperative
   overrun, excluding worker startup and fresh verification; shortest charged
   unfinished target receives the next slice.
5. Complexity reviewed with software-design-philosophy: provisional 5/10 from
   independent review. Information hiding, interface promise documentation,
   newcomer comprehension and coherent ownership need further improvement.
   In particular Executor still orchestrates search/persistence/verification as
   well as supervision, and CLI accesses manifest JSON. Typed identities and
   moving finding transitions into campaign are the specific route to improve
   these diagnostics. No unsupported 10/10 claim is made.

## Plan passes

1. Existing experiment mechanisms selected by knowledge: native transport and
   resource guards retain their regressions; Python internals stay adapter-owned.
2. Runtime isolation checked: product does not import experiment modules; pinned
   interpreter packages are prerequisites rather than runtime repository imports.
3. Cross-task interfaces checked: structured finding APIs accept JSON observations;
   collection emits selected IDs and source/config hashes, campaign builds identity.
4. Test gaps reviewed: six real CLI scenarios now cover restart, fresh operations,
   selection/lifetimes, active timeout, short-budget unexecuted target and hard reject.
5. Remaining acceptance explicitly separated: delegated Linux hard enforcement,
   Windows notification loss, native full matrix and extended longevity remain
   evidence gates. Local tests cannot mark those gates complete.

## Implementation passes

1. CLI-to-worker path exercised on a planted assertion. Pytest object capture and
   FD capture initially broke stdin. Reserving a duplicated descriptor fixed both.
2. Selection checked with -k; discovery initially ran before deselection. Moving
   collection to pytest_collection_finish fixed this and received regressions.
3. Fresh-operation review found hardcoded exits, dropped active shrink evidence
   and missing automatic candidate verification. Exit mapping, append-before-parse
   evidence and fresh fingerprint checks were added.
4. Cross-slice state reviewed: campaign root lock added; elapsed exploration now
   differs from total wall budget; cancellation active candidates remain nonzero.
5. Error presentation reviewed: partial collection errors reject all execution;
   report schema/command and total time fields added; runtime capabilities retain
   hard versus best-effort distinction. Conservative stale locks remain documented.

## Test passes

1. RED/GREEN evidence: storage and worker capture regressions failed before fixes;
   scope of older experimental tests remains attached to their original revisions.
2. Actual subprocess CLI scenarios run against the compiled product, not only
   imported adapter helpers. Windows executable naming fixed to prevent skips.
3. Boundedness checked with proptest for wire/logs and storage caps; scheduler
   tested against 217 Lean-generated cases and an actual finite ASan fuzz campaign.
4. Strict checks run for Rust, Python, docs and workflows; dependency license/source
   checks passed. Native platform checks must be read from their actual CI results.
5. Tests do not establish universal fairness, absence of bugs, complete Python
   semantics or machine-crash durability. Formal model assumptions and benchmark
   limits are recorded alongside their evidence rather than promoted to guarantees.

## Final independent review follow-up

A fresh reviewer identified missing JSON reports on early collection/storage
errors and discarded infrastructure explanations. A subprocess regression first
showed a stale prior-success report surviving an invocation failure. Error paths
now atomically publish schema-1 reports with available context, and observed
infrastructure reasons populate outcome detail. Campaign storage errors preserve
already aggregated target reports. Finding presentation has its own report owner,
showing available rendered values, fingerprints and replay/minimize argument arrays.

Independent final design score was 6.25/10 (five diagnostics passing): typed replay
context/status ownership and further interface simplification remain improvement
opportunities. Product execution promises now replace the old experimental API
comment. No claim of perfect design is made.

Latest scheduler boundary regression reproduced missing-status indexing panic;
unknown status entries are now ineligible. A fresh finite ASan campaign on the
revised scheduler completed 3,395,972 executions in 11 seconds, seed 17, coverage89,
features262, reported RSS468MiB with no failure. The earlier campaign remains
separate evidence about its snapshot.

The final focused recheck confirmed the reporting/detail blockers fixed and
scored the current design 7.5/10 (six of eight diagnostics). Implementation
independence and newcomer comprehension still need typed replay identities and
centralized status/report contracts. Collector rejection reasons are now kept
in the error report rather than replaced with a generic message.
