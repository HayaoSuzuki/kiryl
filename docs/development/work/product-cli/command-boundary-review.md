# Product command boundary review

## Design and alternatives

The previous CLI decoded discovery JSON, assembled worker modes, coordinated saved
finding admission and constructed report metadata. Its interface exposed experiment
choices unused by the product. Moving only helper functions would preserve those
shared decisions. The selected design gives command execution ownership of the
whole product transaction, typed Discovery ownership of collection wire fields,
and executor ownership of guided exploration and closed reproduction modes.
CommandOutcome retains the replay lock through report publication. Experiments
remain separate. Version-1 report/storage formats and CLI arguments stay stable.

## Five design reviews

1. Checked module responsibility: CLI syntax, command transaction, executor process,
   campaign allocation, artifacts durability and report presentation have owners.
2. Compared extraction-only with a typed request; chose user choices without worker flags.
3. Checked temporal decomposition: transaction result retains the lock, not a detached JSON value.
4. Checked speculative generality: removed fixed/baseline modes rather than introducing engines.
5. Checked compatibility: collection budget, source identity, cancellation and output schema retained.

## Five plan reviews

1. Read developer/OKF owners and traced their source references.
2. Split executor/search ownership from command/campaign/report to avoid shared edits.
3. Kept existing subprocess tests as behavior specifications; added discovery/search regressions.
4. Ordered source changes before full Python checks because identity hashes current source.
5. Required strict Rust, Python, documentation and native CI evidence before completion.

## Five implementation reviews

1. CLI no longer decodes nodeids or creates worker settings.
2. Discovery keeps source/environment encoding behind its identity method.
3. Executor exposes exploration and typed reproduction without experiment flags.
4. CommandOutcome owns the saved-finding lock through file publication.
5. Shared output publication preserves atomic replacement and failure cleanup.

## Five test reviews

1. Rust storage, error-report, scheduler and completion regressions cover preserved contracts.
2. Discovery tests cover rejected collection, empty selection and exact identity shape.
3. Search tests distinguish canonical successful inputs from original failure bytes.
4. Actual CLI Python scenarios cover collection, run, replay, minimize and resource handling.
5. Native CI is required separately; local macOS checks cannot establish Linux/Windows enforcement.

## Diagnostic assessment

The CLI-to-command-to-executor path initially failed interface simplicity,
implementation independence, important-decision ownership and newcomer boundaries
(4/8 diagnostics, 5/10). The refactor addresses these with a user-level request,
private decoded discovery and worker settings, closed modes and an owned result.
Interface comments capture transaction lifetime and budget semantics. Design review
and this record make design improvement explicit. Final independent review and
validation results are recorded with the PR; this is not a claim that every module
or every future adapter implementation can change without its contract changing.

## Final observations

Independent read-only integration review found a compatibility difference for
successful collection with zero supported nodes. Discovery now exposes semantic
selection completion before admission; adapter errors remain incomplete. Exact
legacy diagnostics were restored. All eight diagnostics pass for the reviewed
product command path (10/10); this is not a repository-wide perfection claim.
The large private supervision state machine and dynamic version-1 output remain
implementation complexity, hidden behind operation and publication boundaries.
Local verification: 38 Rust tests, strict Clippy/fmt/rustdoc, 72 product Python
passes with four platform skips, 28 documentation tests and strict site build.
Native results for this revision belong to PR checks rather than earlier snapshots.
