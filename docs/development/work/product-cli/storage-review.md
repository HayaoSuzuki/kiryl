# Product storage review evidence

Scope: `src/artifacts.rs` and `tests/artifact_store.rs` in the product CLI
working tree, reviewed on 2026-11-11. This is implementation evidence, not native
multi-platform acceptance or a power-loss durability guarantee.

## Stage 1: namespace, immutable publication and bounded snapshots

Implementation review passes:

1. Identity/schema: exact version-1 manifest comparison rejects changed target
   variants and environment values. Found manifest reads were unbounded; changed
   them to a limited reader and capped serialized identity at 1 MiB.
2. Namespace failure handling: found directory enumeration errors were ignored
   during missing-manifest detection. Propagate every entry error; existing data
   never receives an inferred replacement identity.
3. Publication: found primitive originals were visible while being written.
   Publish a synced temporary file with a non-replacing hard link, then remove
   its temporary name. Existing originals survive ID collisions and reopen.
4. Corpus bounds: found metadata-only size checking could permit growth between
   metadata inspection and parsing. Use the same limited reader for snapshots;
   enforce 256 entries, 4096 bytes per input and a 5 MiB encoded-file cap.
5. Failure/ownership: confirmed exclusive create-new lock acquisition, lock
   release after manifest rejection, and old snapshot preservation when a pending
   filename prevents a new write. No destructive stale-lock recovery was added.

Test review passes:

1. Isolation: found roots named only with PID; added a process-local atomic
   sequence so parallel tests cannot erase another test's store.
2. Identity sensitivity: changed mismatch assertion to differ specifically in
   parameter variant (`[a]` versus `[b]`), retaining original-byte assertions.
3. Boundary sensitivity: added exact maximum corpus round trip, oversized input,
   oversized count and corrupt/out-of-range corpus checks without automatic repair.
4. Failure sensitivity: added pending snapshot obstruction with prior snapshot
   equality and pending original obstruction with absent final filename. These
   inject publication failures; they do not simulate actual disk-full hardware.
5. Persistence sensitivity: retained reopen collision checks, invalid derivative
   names/directories and moved-destination failure assertions. These compare
   original bytes after failure, rather than only checking error returns.

## Stage 2: controller-facing structured findings

Implementation review passes:

1. Contract completeness: added `record_original(test_id, input, observation)`
   containing version, exact test ID, complete raw byte array and observation.
   `finding-N.bin` stores JSON separately from raw `original-N.bin` primitives.
2. Validation: capped test IDs at 16 KiB, inputs at 4096 bytes and serialized
   finding/evidence records at 1 MiB. Empty test IDs fail before publication.
3. Provenance: added `derived(original, evidence)` requiring an in-store regular
   `finding-N.bin`, version 1, valid nonempty test metadata, byte-array input and
   an observation field. Evidence includes original filename and test ID.
4. Immutability/error behavior: derived writes use separate `evidence-N.bin`
   publication and never rewrite originals. Symlink originals are rejected;
   raw and derived artifact paths cannot be passed as structured originals.
5. Interface review: kept existing public APIs; new APIs return UTF-8 absolute
   paths for controller integration. Found path conversion could fail after
   publication; reject a non-UTF-8 store path before creating a finding.

Test review passes:

1. Completeness: assert exact JSON equality for the original, including binary
   bytes `[0,255]` and initial crash observation.
2. Original separation: compare bytes before/after derived publication and
   reject raw-original and evidence paths as structured-original references.
3. Version sensitivity: corrupt an original to version 2 and require failure;
   also reject absent observation, empty test ID and input byte 256.
4. Path safety: add Unix symlink rejection. This does not claim adversarial
   concurrent directory mutation is prevented; the campaign lock serializes
   cooperating writers only.
5. Failure sensitivity: inspect the preexisting pending file after a failed
   publication, require no final original, and prove a later distinct finding
   can still publish. Existing originals are checked after manifest rejection.

## Commands and limits

- `cargo test --offline --locked --test artifact_store`: 11 passed, 0 failed.
- `rustfmt --edition 2024 src/artifacts.rs tests/artifact_store.rs`: passed.
- `cargo clippy --offline --locked --test artifact_store -- -D warnings`:
  passed after fixing the new `assert_is_empty` lint in the boundary test.
  The first run was blocked by diagnostics in independently changing
  `src/campaign.rs` and `src/resource/mod.rs`; these subsequently cleared.
- Initial test execution before implementation was blocked by dependency-index
  DNS failure, then by the temporarily stale lockfile. Therefore the added APIs
  have passing regression evidence, but no claimed observed pre-fix red run.

Ruling: retain `io::Result` for compatibility with existing callers rather than
introducing a second error hierarchy within this storage-only task. Serialized
versions and invalid metadata are rejected explicitly; callers treat failures as
infrastructure errors. The cost is that callers distinguish details through
error messages rather than public enum variants.

No API silently repairs incompatible manifests, malformed findings or corrupt
corpus data. Atomic publication and synced file contents do not establish
filesystem-directory durability across power loss. Native Windows/Linux behavior
and actual disk-full injection remain separate acceptance work.
