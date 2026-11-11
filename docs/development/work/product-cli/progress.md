# Current status

Implementation, final review fixes and all native acceptance checks completed at
source revision `795fe6e`. See [the native matrix record](evidence/native-matrix.json)
for the six product OS/interpreter cells, four hard-resource gates, Rust/Python
quality, formal/fuzz checks, documentation and dependency audit success. Earlier
entries below are historical increments, not the current feature inventory.

# Product CLI progress

Working branch: feat/product-cli. Plan: docs/development/work/product-cli/implementation-plan.md.

Task 1 is in progress, not complete. No product CLI exists yet.

Initial artifact primitive RED: unresolved kiryl::artifacts import.
GREEN: cargo test --locked --test artifact_store (1 passed).
Quality: cargo clippy --locked --workspace --all-targets --all-features -- -D warnings passed.

Implementation reviews (initial primitive only):
1. Scope: byte storage is implemented; version manifests and environment identities remain absent.
2. Errors: create_new prevents overwriting; failed writes remove only newly created paths. Fault injection remains required.
3. State: writer lock is exclusive and removed on Drop. Crash-stale lock recovery remains manual and undocumented.
4. Sensitivity: test checks exact bytes, distinct derivatives, exclusive open and reopened allocation; corrupt/foreign paths need regressions.
5. Design: provisional 6.25/10 (five diagnostics pass). Interface simplicity, change isolation and newcomer comprehension need re-evaluation after versioned metadata exists. File naming currently exposes linkage; consolidate behind typed IDs before campaign integration.

Test reviews (initial primitive only):
1. Scope: one integration test covers the initial byte persistence contract, not Task 1 acceptance.
2. Boundary: empty/large data and exhausted IDs not tested.
3. Side effects: temporary directory is PID-specific; cleanup occurs after store drop. Split cases should use unique temporary roots.
4. Sensitivity: assertions compare original bytes after derivative creation and reopen. Lock refusal is checked while owner remains alive.
5. Evidence: local Rust only; no OS durability, disk-full, symlink/security or machine-crash guarantee established.

Next: complete typed/versioned manifests, environment/variant identity, bounded corpus and write-failure injection. Update existing OKF owner before committing product implementation. Then execute Tasks 2–5 without additional approval gates.

Commit attempt failed: sandbox denied worktree git index.lock. Retry commit with scoped escalation; no auto-review rejection has occurred.

## Review follow-up

Fresh independent review identified directory acceptance and relative-root drift.
Regression for directory acceptance failed before the fix, then passed after
requiring a regular, non-symlink original with a numeric record name. Store roots
are now canonicalized. Missing manifests remain a Task 1 blocker, not a passed
acceptance check. PID-only test directories remain a minor test-isolation limit.

Latest local verification: Rust integration 3 passed; strict Clippy passed;
Python documentation tests 28 passed; documentation checker and strict MkDocs
build passed. No product CLI/platform acceptance or fuzz campaign was executed.

PR is a draft for the initial storage primitive, references #5 without closing
it, and does not represent completion of the approved implementation plan.

## Manifest/corpus increment

Added open_matching with strict version+identity equality and refusal to adopt
pre-existing unversioned data. Identity content is caller-owned; automatic source
and interpreter identity generation remains unimplemented. Added corpus snapshot
limits (256 entries, 4096 bytes/input, 5 MiB encoded read cap). Failed validation
preserves the previous snapshot. No CLI integration yet.

Implementation self-review passes:
1. Requirements: schema and supplied target/variant/environment comparison now
   exist; campaign-generated identities remain a gap.
2. Errors: manifest mismatch releases writer ownership through Drop and preserves
   originals; pre-existing pending files fail conservatively.
3. Side effects: hard-link publication prevents replacing a manifest; corpus uses
   rename after closing its file. No machine-crash/directory-fsync claim.
4. Sensitivity: tests exercise mismatching identities, schema 2 and corpus count
   overflow while checking previous bytes remain unchanged.
5. Design: provisional 6.25/10 remains; typed IDs, semantic identity ownership and
   newcomer-facing guarantees still fail the earlier diagnostics. Keep these
   requirements in Task 1 rather than adding a generic storage framework.

Test self-review passes:
1. Scope: five integration tests exercise primitives; Task 1 still incomplete.
2. Boundaries: 256/257 count boundary covered at rejection; per-input length and
   malformed encoded corpus still need dedicated tests.
3. State: reopen and mismatch paths demonstrate lock release and original retention.
4. Sensitivity: new tests failed with missing APIs before implementation; unsupported
   schema must never be accepted merely because the identity matches.
5. Evidence: local macOS Rust only; native Windows replacement, injected partial
   writes, Linux disk-full and fuzz/property/formal checks remain pending.
