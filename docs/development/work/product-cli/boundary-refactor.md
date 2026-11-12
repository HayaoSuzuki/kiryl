# Storage and completion boundary refactor

The previous independent review scored the product integration 7.5/10: changing
storage representations affected callers, and newcomers needed implementation
knowledge to understand boundaries. This change targets those two diagnostics.
It preserves the CLI, JSON field names and version-1 storage envelope.

## Decisions

Storage owns admission through ReplayContext: bounded typed manifest decoding,
recorded selection, environment comparison, original/variant consistency and
campaign lock lifetime. The CLI asks for a reproduction context rather than
knowing manifest filenames or nested identity keys. SavedFinding and
FindingObservation expose admitted semantic data to execution; raw record loading
is private. FindingDescription similarly supplies presentation without exposing
the original envelope. These are stable concepts rather than a generic codec API.

Report owns Completion and its precedence, observation classification, target
stopping policy and both report shapes' exit mapping. Duplicate CLI decisions
and unused forwarding exit methods were removed. Python wire-specific handling
remains executor-owned; this change does not introduce an engine abstraction.

A generic storage plugin/visitor or a shared JSON-path utility would still expose
representation choices and add interfaces without hiding the decisions. The
selected context instead enforces admission and writer lifetime as one operation.

## Five design review passes

1. Responsibilities: storage admits evidence, execution consumes semantic data,
   report classifies outcomes and CLI dispatches user operations.
2. Information hiding: CLI manifest reads and report original-envelope reads were
   removed; execution no longer indexes stored JSON.
3. Interface depth: ReplayContext hides version, namespace, identity and lock rules;
   no arbitrary codec configuration or lifecycle flags were introduced.
4. Generality: one completion contract serves standalone and campaign results;
   candidate preservation and interruption priority remain explicit.
5. Comments: public interfaces document admitted evidence, lifetime, precedence
   and errors rather than restating individual JSON operations.

## Five plan review passes

1. Scope fixed to the earlier review's failed diagnostics; no new CLI options.
2. Existing persisted shape retained, avoiding a migration masquerading as refactoring.
3. Admission/context lifetime reviewed before caller integration to prevent double locks.
4. Added tests for both report shapes, priority and replay identity/namespace failures.
5. Existing product E2E and native resource jobs remain the acceptance checks;
   old native observations retain their original revision rather than proving this diff.

## Five implementation review passes

1. Root ownership survives collection, execution, presentation and report publication.
2. Temporary target admission lock is released before execution reacquires it.
3. Bounded original reading is shared by derivation, replay and finding presentation.
4. Type validation rejects malformed semantic fields before executing an original.
5. Removed forwarding methods; storage decisions are not relocated into CLI helpers.

## Five test review passes

1. The new shared-exit API test first failed to compile before implementation.
2. Replay context regression exercises recorded selection, identity comparison,
   active writer rejection and lock release.
3. Additional admission regression rejects schema/variant mismatch and compares
   unchanged original bytes after failure.
4. Exit regressions cover cancellation with/without an active candidate, finding,
   infrastructure and interruption priority in standalone and campaign shapes.
5. Full Rust and product Python suites exercise actual subprocess behavior;
   formatting, strict Clippy and rustdoc remain required before publishing.

## Independent review

A fresh reviewer found no critical or important correctness defects and scored
these targeted storage/status boundaries 10/10: all eight diagnostics pass, no
failed rows. This is a scoped design assessment, not a claim that the entire
repository is perfect. The reviewer checked code and regression evidence; the
coordinator runs the full suites and native CI.

Early command errors also go through report::record_error and Completion; the
CLI no longer repeats IO-error-to-exit encoding. A regression first failed with
the absent API, then checked interruption, pre-case budget expiry, infrastructure,
retained context and elapsed time. This closes the early-error path as well as
normal standalone/campaign completion without altering user-visible semantics.
