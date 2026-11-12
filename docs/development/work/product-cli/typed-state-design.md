# Typed state exchanges

This records the maintainer's expanded requirement received during the session
operation refactor: all active state exchanges use enums or domain types, with
string conversion confined to boundaries. It covers both the product and the
standalone feasibility implementation, without coupling their packages.

## Design and compatibility

| Domain | Internal representation | Boundary |
| --- | --- | --- |
| Execution classification | Rust OutcomeKind; Python ResultKind/OutcomeKind | JSON labels |
| Verification | Closed verdict or explicit diagnostic wrapper | Existing flat string |
| Operations/report modes | Token-carrying session enum and report-mode enums | CLI/start/report encoding |
| Control frames | Typed incoming/outgoing discriminators | Strict string JSON kind |
| Resource identity | BackendName/ResourceBackend; typed capabilities | Unchanged capability objects |
| Adapter lifecycle | PytestPhase; fixture names remain identifiers | Pytest hook reports |
| Comparison/evidence | BenchmarkMode, ComparisonTarget, ProbeStatus, SummaryStatus | CLI/JUnit/evidence JSON |
| Artifact family | ArtifactKind | Existing filename prefixes |

Known labels are enums. Extensible unknown labels and free-form diagnostics are
explicit data-bearing variants/wrappers, not implicit successful states. IDs,
paths, fingerprints, feature names and diagnostic messages remain strings because
they are unbounded data. Booleans and optional capabilities retain their own
meaningful types. No string-equality or Deref compatibility shim is introduced
for domain enums. Tests of encoded JSON retain literal external-format oracles.

Rust saved unknown classifications retain their exact version-1 identity. Missing
kind/verification defaults remain empty labels. Missing semantic tokens cannot be
constructed; empty byte tokens remain valid. Control-kind input must be a string:
serde's externally tagged enum object syntax must not admit an object as a state.
Python JSON reconstruction restores actual enums rather than relying on field
annotations. Human diagnostics explicitly encode labels, avoiding Enum repr leaks.

Alternatives considered: Literal-only aliases leave runtime strings circulating;
a generated universal schema adds build/dependency complexity and couples the
independent experiment to the product. Local types own each package's domain and
codecs, with literal boundary and integration tests preserving interoperability.
No engine, protocol version, user CLI or storage schema is changed.

## Implementation and plan reviews

| Pass | Design | Plan | Implementation | Tests |
| --- | --- | --- | --- | --- |
|1 inventory|Separate states from identifiers|Assign nonoverlapping language/package ownership|Type producers and consumers together|Compile-red migration and literal codecs|
|2 extensibility|Unknown labels remain opaque|Preserve default and diagnostic behavior|Other/Diagnostic wrappers preserve strings|Known/unknown/empty/non-string cases|
|3 boundaries|One codec owns each vocabulary|Keep experiment standalone|Strict incoming tag shape and canonical output|Object-discriminator and Enum-repr regressions|
|4 semantics|Retain supervision and evidence policy|Preserve generated oracle connection|No lock/retention/resource policy changes|Existing native fault/property/formal cases|
|5 acceptance|No speculative framework|Full suites and native exact gate|No state-string escape hatches|No exclusions or rounded percentages|

These five passes were repeated for each phase by the implementers. Independent
read-only review initially passed seven of eight diagnostics (8.75/10 scoped):
the experiment benchmark's closed outcome decoder disagreed with the extensible
controller contract. It also found lost Lean corpus consumption after test
splitting. The latter was treated as Important because it weakened implementation
correspondence, regardless of the reviewer's Minor label.

## Review fixes and evidence

Unknown-only and unknown-before-failure benchmark scenarios failed with ValueError
before the fix. The decoder now returns a known enum or UnknownOutcome wrapper;
malformed non-string labels reject explicitly. Dedicated shape tests were observed
failing without that guard, then all 42 model/benchmark cases passed.

Each budget testcase now consumes one generated oracle row. A separate cardinality
case detects uncovered added rows. Changing one oracle row deliberately made its
Rust testcase fail; the original bytes were restored before the whole suite passed.
No independent testcase table is hidden inside a helper loop.

The failed information-hiding diagnostic is addressed by the extensible decoder;
the author recheck passes all eight rows (10/10 for these state boundaries only).
This is not a whole-repository score or a native coverage acceptance claim.

Before native collection: product Rust 377 tests pass; the full product Python
run passes 275 tests with six native-only skips. Host/Linux/Windows strict Clippy,
217 formal scheduler rows and fuzz compilation pass. The isolated experiment's
Rust suite passes 54 tests; Python passes 183 tests with six native-only skips after the review fixes. Ruff ALL, formatting,
strict ty across six OS/interpreter target cells and AST test-shape checks apply
to both Python implementations. Model-only coverage and one selected hoimin mutant
are supporting evidence, not whole-experiment coverage or mutation saturation.

The current six-cell product native gate and PR checks remain required. Earlier
accepted coverage belongs to its recorded revision. User guidance needs no change
because observable command, wire and storage contracts remain unchanged.

The preceding Windows 3.14 CI run exposed a pre-existing acceptance timing race:
the two-second campaign could expire before Python completed startup, so no
active case existed to time out. The test now allows eight seconds for the
campaign while retaining its 0.1-second case limit. The unchanged assertion
passes locally; the new native run must confirm Windows behavior. Domain test
helpers also exchange OutcomeKind instead of string labels; Rust checks pass
after that final fixture migration.

Native experiment CI then exposed an unrelated deadline coupling in the fixture
setup test (macOS 3.14): a five-second campaign could end during fresh replay,
leaving verification pending. Deliberately increasing setup duration reproduced
the same pending result locally; removing only the campaign limit confirmed the
original failure. The acceptance test now parameterizes normal and 2.5-second
setup, retains its 150 ms case limit, and isolates the case contract from campaign
expiry. The slow testcase failed with the old deadline (5.32 s), and both cases
passed after the fix (9.80 s total). This changes test supervision only.

All six native product cells passed at 39579e4, but the union inventory rejected
lib.rs: its new pub-use declarations have no executable LLVM locations, while
the old classifier recognized only module declarations. Two literal declaration
cases reproduced the rejection before the fix. The inventory now recognizes
single-line import/re-export declarations too; separate function, function after
re-export, and same-line function cases all continue to require library coverage.
All 67 coverage-tool tests pass. No executable location or test is excluded.
The final experimental probe suite passes 184 tests with six native-only skips.
