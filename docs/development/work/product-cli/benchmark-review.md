# Local product longevity and ordinary Hypothesis comparison

This is a finite local observation, not a performance ranking or native-platform
acceptance. The distributable fixture has nested integer application branches
and asserts true arithmetic identities. It contains no planted bug, known failing
seed, account data or external service. Copyright and licensing follow this
repository's Elastic-2.0 terms.

## Method and reproduction

Run from the repository root after building the product CLI and syncing the
product Python environment:

```sh
python3 docs/development/work/product-cli/evidence/benchmark-run.py
```

On this macOS machine, process-table access requires running this command outside
the restricted execution sandbox. An initial sandbox attempt failed with
`PermissionError` from `ps`; it is infrastructure failure, not benchmark evidence.
The approved native invocation successfully sampled process trees.

The harness runs product and ordinary Hypothesis sequentially with seed 17 and
requested 30-second budgets, isolated temporary source roots, fresh separate
product corpus and Hypothesis example database. Product uses the real CLI with
explicit `--best-effort`. Ordinary Hypothesis calls the same property directly
with `max_examples=10**9`, no per-example deadline, and an isolated
`DirectoryBasedExampleDatabase`. At 30 seconds the harness sends ordinary
Hypothesis SIGINT, allowing its final metrics to flush. Both phases include
startup in observed wall time; finalization and the 0.5-second sampling interval
can extend observation beyond the requested budget.

The application fixture is copied to `application.py`; the Hypothesis test is
copied to `test_branches.py`. An atexit handler writes one session summary rather
than per-example disk writes. Counts are actual property calls, separate from
product attempt counters (which also include rejected/generated inputs).
Property path labels are an explicit finite diagnostic, not a general coverage
measurement.

Each half-second sample calls `ps` and sums RSS for the root process and its
currently visible descendants. This reports sampled tree RSS, not unique
physical memory, native enforced limits or a true unsampled peak. Shared pages
can be counted more than once. Disk samples count the entire temporary workspace,
including fixtures, caches and session summaries; the final product artifact
inventory and ordinary database size are reported separately. Sampling itself
has overhead. No resource limit or steady-state memory bound follows from a
30-second run.

Exact commands, wall times, samples, property paths, environment, revision and
SHA-256 snapshots of the binary, Rust sources, adapter sources, locks and fixture
are in [the result](evidence/benchmark-results.json).
The full product JSON is [retained separately](evidence/benchmark-product-report.json).
Source/harness files are [the driver](evidence/benchmark-run.py),
[the test fixture](evidence/benchmark-fixture.py) and
[the application fixture](evidence/benchmark-application.py).

## Primary observations

Host: macOS 15.7.7 arm64; CPython 3.14.9, Hypothesis 6.170.13,
pytest 9.2.0. Base revision `f6aa6cd660da3e6b43b87fa40c3e14777e8e6312`
plus the recorded working tree. The executable SHA-256 at launch was
`694b581f778ff100c5870432f68ac007520222a8cad93f681bb9010814044671`.

| Observation | Product | Ordinary Hypothesis |
| --- | ---: | ---: |
| Requested phase budget | 30 s | 30 s |
| Observed wall time including finalization | 30.173 s | 31.277 s |
| Campaign attempt counter | 139,640 | not applicable |
| Recorded property calls | 105,537 | 172,323 |
| Recorded classifier labels | 7 / 8 | 8 / 8 |
| Peak sampled tree RSS | 63,664 KiB | 216,320 KiB |
| RSS samples | 58 | 60 |
| Final workspace bytes | 18,675 | 6,037 |
| Final artifact store / explicit database | 3,542 bytes | 0 bytes |
| Process exit | 1 | 0 after caught SIGINT |

Product's last slice reports 13 application branches and 114 corpus entries.
The artifact inventory contains a 2,788-byte corpus, 498-byte manifest and
256-byte original record. There was no observed assertion failure. At the budget
boundary an active case remained unverified: the `campaign_cancelled` outcome
retains its original reference, and the CLI intentionally returns 1 for that
unresolved candidate. This differs from a completed passing campaign and must
not be reported as a discovered property defect. The temporary artifact content
was not archived; the inventory, original reference and full report are retained.
The fixture and exact commands support a fresh reproduction rather than replay
of that removed temporary original.

Session metrics comprise 26 product worker summaries and one ordinary summary.
They count calls that reached the property; rejected input attempts need not call
it. Summaries may miss a worker forcibly terminated before atexit, so recorded
product calls and labels are observations rather than an assertion that no
additional call ever occurred. No per-trial history is accumulated by the
benchmark: each worker retains only a counter and at most eight labels. Product's
retained final report has 257 outcomes (bounded recent outcomes plus cancellation)
and one log tail; this single observation does not prove a permanent bound.

This pair shows fewer recorded calls and fewer fixture labels for product under
these settings. RSS was lower in the sampled product tree, but separate process
lifetimes, Python settings and sampling overhead prevent inferring a general
memory advantage. Neither phase discovered a property bug. No throughput,
discovery or resource-superiority claim follows.

The binary on disk was replaced by a concurrent build during the observation;
its ending SHA is disclosed in the result. The running process used the image
launched at the starting SHA. Recorded Rust, adapter, fixture and lock hashes
remained unchanged. Treat this as evidence for that launched working-tree binary,
not automatically for the later binary or subsequent source revisions.

## Pilot and interpretation limits

The first completed pair placed branches entirely in the test module. Product's
application-coverage filter reported zero branches, so that pair does not assess
application-guided exploration. It is preserved under `benchmark-pilot-*` and
excluded from the primary result. The correction moved only the classifier into
an imported application module and repeated both complete 30-second phases.

The pilot observed no failures: product 105,797 property calls and six path
labels, ordinary 175,934 calls and eight labels. Its sampled peak tree RSS was
63,232 KiB versus 228,432 KiB. Those numbers are fixture-specific and do not
establish speed, memory or discovery superiority. The pilot's binary file was
replaced by a concurrent build; its starting and ending hashes disclose that
change. The already-running process retained its launched executable, and all
recorded Rust/adapter source hashes were unchanged.

## Review passes

1. Scope: use real native product and ordinary Hypothesis executions with a
   public pure fixture; exclude known bug seeds and avoid superiority claims.
2. Isolation: fresh independent temporary roots, seed 17, explicit duration,
   separate corpus/database and sequential execution. Ordinary is stopped by a
   monotonic harness deadline rather than assuming `max_examples` is elapsed time.
3. Observation accuracy: sample descendant tree RSS with `ps`, record all samples,
   distinguish workspace bytes from stored corpus bytes and property calls from
   campaign attempts. Record startup/finalization and sampling limitations.
4. Feedback validity: found the pilot's test-module classifier produced no
   application coverage. Preserve the pilot, move classifier into application
   code and repeat both phases; never recast zero coverage as guided evidence.
5. Provenance and negative results: retain commands, source hashes and full JSON;
   record no discovered failures as a negative observation. No long-run proof,
   platform-support claim or hard resource gate is inferred from this experiment.

## Test and evidence review passes

1. Fixture behavior: parsed all three Python sources and evaluated explicit
   integer boundaries to exercise all eight classifier labels. Assertions are
   arithmetic identities, not planted defects or a hidden known-failure seed.
2. Measurement sensitivity: store inventory separates corpus/manifest/original
   from workspace caches. The ordinary explicit example database was empty;
   zero bytes is recorded rather than inventing saved failures.
3. Failure classification: inspected final product outcomes and original
   references. An unresolved campaign-boundary candidate explains exit 1;
   no assertion bug or completed passing status is inferred from it.
4. Revision sensitivity: compared starting/ending hashes and found executable
   replacement; recorded this instead of claiming a single unchanged binary on
   disk. Source and dependency hashes remained stable within the primary pair.
5. Result interpretation: retained both primary and excluded pilot results,
   full RSS samples and negative/no-bug observations. Classified the blocked
   sandbox attempt as infrastructure failure and the missing archived temporary
   original as an evidence limit, not as a successful replay.
