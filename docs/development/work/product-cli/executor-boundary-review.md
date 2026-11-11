# Executor product boundary review

Scope: `src/executor.rs` and `src/search.rs`, based on `ca09700` in the
`work/product-design` workspace. This record supports the owning
[fuzzing architecture decision](../../knowledge/fuzzing-architecture.md).
The parent integration owns that concept's metadata and caller changes.

The former Rust configuration admitted fixed inputs, unguided mutation and an
ordinary Hypothesis baseline although the product campaign always selected guided
search. Those experimental choices leaked through the product execution boundary.
The experiment directories and Python baseline helpers remain separate evidence;
this refactor removes those choices only from product Rust.

## Design reviews

1. Responsibility: execution owns one guided slice and automatic verification;
   campaign allocation remains with the campaign module.
2. Information hiding: `ExecutionSettings` contains interpreter, limits and artifact
   identity; callers cannot choose search internals or submit raw invocation lists.
3. Alternative: retaining boolean switches would preserve unsupported combinations;
   removing them makes the product contract explicit.
4. Reproduction: `Reproduction` closes the caller's operation choices while raw
   replay selection remains internal and follows the persisted original.
5. Collection: `Discovery` owns wire decoding, admission and identity construction;
   callers use targets and environment without duplicating Python JSON keys.

## Plan reviews

1. Change the collection boundary with a failing regression before implementation.
2. Remove the unused outer invocation/offset loops while retaining one supervised
   session and its teardown path.
3. Preserve guided random state and feedback observation order exactly.
4. Retain fresh replay, shrink candidates, pending evidence and original lineage;
   remove no resource supervision or fixture handshake.
5. Preserve serialized public report types, including the reserved null
   `baseline_statistics`; integration tests must cover the complete CLI.

## Implementation reviews

1. Guided search still starts with the restored corpus's first entry; mutation and
   novelty bounds are unchanged. The feedback flag was always true on this path.
2. Raw reproduction admits a single retained input using a private optional input
   and dispatch flag; fixed-input indexing and offset consumption are gone.
3. Automatic failure replay, confirmed-only minimization and candidate rechecks
   retain the previous cancellation checks and durable derived records.
4. Native crash and timeout originals still survive termination and receive a
   fresh raw replay; OOM observation and process-tree cleanup remain unchanged.
5. Bounded collection still attaches containment before importing project code,
   caps stdout, retains stderr and terminates the tree before returning discovery.

## Test reviews

1. The Discovery regression initially failed compilation because `Discovery` did
   not exist, establishing the missing boundary before implementation.
2. Discovery tests compare the complete identity shape and reject explicit errors
   and empty supported selection; environment and target accessors are exercised.
3. Search regression checks canonical bytes for a novel passing input, original
   bytes for a novel failure, and rejection exclusion from corpus/feature counts.
4. The reserved report field and the two reproduction wire modes are checked;
   existing property tests cover exact log suffix retention and merge accounting.
5. Read-only integration review found that successful empty collection must keep
   `selection_complete=true` while explicit adapter errors keep it false. A semantic
   `selection_complete()` accessor preserves that distinction without exposing wire
   fields or interpreting diagnostic strings. Exact existing diagnostics are tested.
   Early library runs were blocked by obsolete campaign imports and its `run` call;
   after caller migration, `cargo test --locked --lib` passed all 15 tests and the
   workspace all-target/all-feature Clippy check passed with warnings denied.
   These checks do not establish native platform support.

## Software-design diagnostic

The eight questions from `software-design-philosophy` are assessed as a design
review aid, not a measured quality metric. Before: 4/8 affirmative (single-sentence
responsibility, interface comments, explicit review and design investment).
After: 8/8 affirmative within this scope: execution and discovery have describable
purposes; their interfaces are smaller than their mechanisms; search policy and
collection wire keys can change without caller changes; interface comments state
promises; this review considers complexity; modules hide execution and collection
decisions; callers can understand the contracts without inspecting the transport;
and the change spends explicit effort removing unnecessary product choices.

Residual complexity: the private session state machine remains large because case
correlation, active-input retention, deadlines and resource evidence share state.
Its size is not evidence of native compatibility. Windows and Linux hard-resource
acceptance still require their actual configured environments.

Final integration review also inspected `command.rs`, `main.rs` and `campaign.rs`
read-only: the replay lock remains alive through publication, collection consumes
execution budget, typed campaign progress retains its serialized shape, and the
selection completion edge above was corrected before the final verification run.
