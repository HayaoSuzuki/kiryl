# Finite 120-second product observation

This observation exercises the real product CLI against the same imported
integer-branch application as the earlier comparison, with seed 17 and a fresh
isolated corpus. A finite run can expose growing memory/history but cannot
establish a mathematical bound, steady state, hard resource enforcement or
cross-platform compatibility.

## Method and retained evidence

```sh
python3 docs/development/work/product-cli/evidence/benchmark-long-run.py
```

The harness uses the product Python environment and an explicitly best-effort
120-second CLI campaign. It samples RSS of the visible process tree with `ps`
once per second, alongside artifact-store bytes/files, corpus entry/input-byte
counts and completed-worker summaries. Sampling includes root and descendant
resident sets and can double-count shared pages; short peaks between samples can
be missed. Process inspection required an approved invocation outside the local
restricted sandbox. No hard memory capability is inferred.

Actual wall time includes startup and finalization. A separate 140-second
external allowance kills the process group if the CLI does not finalize; this is
an observation safety cap, not the product's own campaign budget. The harness
retains at most one sample per second during that finite allowance. Each fixture
worker stores one scalar call counter and at most eight branch labels, writing
one summary at exit. A killed worker may omit its summary: property-call counts
are recorded observations rather than proof of all actual evaluations.

The final store is archived in
[benchmark-long-store](evidence/benchmark-long-store/target-0/manifest.json),
including any original finding bytes, instead of deleting it with the temporary
workspace. Full samples, source hashes, commands, environment and final report
are in [benchmark-long-results.json](evidence/benchmark-long-results.json).
The [raw product report](evidence/benchmark-long-product-report.json) is also
retained. Earlier comparison evidence was not edited.

## Actual observations

Host: macOS arm64; CPython 3.14.9, Hypothesis 6.170.13, pytest 9.2.0.
Base revision: `f213462570d6081904ac2d0472665f755e116efe` with recorded working
tree. Launched executable SHA-256:
`6d2730d70103b45f8d31ec1c2633f8961f41f75ff601798d1323b4950d458265`.

| Observation | Result |
| --- | ---: |
| Requested duration | 120 s |
| Harness wall time | 120.405 s |
| Product total elapsed | 120.015 s |
| Product campaign elapsed | 119.832 s |
| Exit code | 0 |
| Campaign attempts | 553,776 |
| Cumulative useful / rejected | 433,596 / 120,180 |
| Recorded property calls | 431,696 |
| Completed worker summaries | 104 |
| Recorded classifier labels | 8 / 8 |
| Sample count | 118 |
| Peak sampled tree RSS | 110,064 KiB |
| Final corpus entries / raw input bytes | 256 / 1,024 |
| Final store bytes | 6,509 |
| Maximum sampled store bytes | 10,596 |

The corpus first reached its configured 256-entry capacity in the 68.362-second
sample. All subsequent valid corpus samples remained at or below that capacity.
The final archived store has a 4,141-byte `corpus.json` and 2,368-byte manifest;
no original finding was produced. Transient store samples can include writer
locks and a pending snapshot alongside the prior snapshot, explaining why peak
sampled store bytes exceed final bytes. Corpus source limits are 256 entries and
4,096 bytes per input; this fixture uses four-byte raw integer inputs.

The target recorded 140,187 output bytes while retaining exactly 65,536 bytes
with `truncated=true`, matching the selected retention cap. Its last worker
report retained 257 outcomes (recent bounded outcomes plus cancellation) and one
313-byte worker log tail. It reported 13 application branches and 256 corpus
entries. The final campaign-cancelled outcome had no active original, and the
campaign returned 0; this is distinct from the unresolved original in the prior
30-second comparison. No property assertion defect was observed.

| Sample window | Minimum / maximum sampled tree RSS | Maximum corpus count |
| --- | ---: | ---: |
| 0–30 s | 2,720 / 83,920 KiB | 103 |
| 30–60 s | 31,264 / 109,840 KiB | 215 |
| 60–90 s | 32,848 / 103,680 KiB | 256 |
| 90–120 s | 56,720 / 110,064 KiB | 256 |

RSS varies with worker lifetime and sampled startup/teardown. These windows show
neither an unbounded-growth proof nor a steady-state guarantee. They show that
this particular two-minute run continued through over half a million attempts
with the observed corpus/output caps and finite sampled RSS. Product attempts,
useful inputs and fixture calls differ because rejected inputs do not reach the
property and a worker can stop before its final summary; the metrics are not
interchangeable.

Starting/ending snapshots changed only the binary file on disk, replaced by a
concurrent build. Rust/adapter sources, dependency locks and fixture/harness
hashes remained stable. The running process retained its launched executable;
these observations apply to its starting SHA rather than the replacement file.
The best-effort portable backend reported no native hard tree-memory or cleanup
capability. The separate 140-second safety kill was not needed.

## Implementation review passes

1. Scope: use the existing public fixture and real CLI; no production-code change
   or artificial known failure seed. A separate store prevents reuse of earlier
   corpora and findings.
2. Budget: requested 120 seconds with seed 17; independent 140-second safety cap
   and cleanup on sampler exceptions prevent an abandoned unbounded campaign.
3. Memory: sample the entire currently visible process tree instead of using
   cumulative `getrusage` from unrelated phases. Keep the shared-page and sampling
   limitations explicit.
4. Disk/progress: record actual corpus count and decoded input bytes separately
   from serialized store bytes. Completed session counters do not require
   per-trial disk writes or an ever-growing in-process trial history.
5. Provenance: snapshot executable, Rust/adapter sources, locks and harness before
   and after; archive all store bytes before deleting temporary source roots.

## Evidence review passes

1. Syntax: harness AST parsing passed before execution.
2. Counters: compare campaign attempts with worker summaries without assuming
   rejected generated inputs reached the property.
3. Retention: inspect final outcome/log lengths and corpus files; relate observed
   counts to source caps without claiming a finite observation proves them.
4. Status: distinguish assertion defects, unresolved campaign-boundary originals
   and infrastructure failures; preserve originals for inspection.
5. Revision: compare starting/ending hashes and report any source or executable
   change. Evidence applies to the launched working-tree binary and recorded
   environment, not automatically to later commits.
