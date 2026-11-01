---
type: Audit
title: Per-case Python branch feedback experiment
description: Observed application attribution, monitoring ownership, coexistence limits, and paired timing.
status: draft
sources:
- id: results
  resource: ../work/coverage-guided-python-fuzzing/feasibility-results.md
  revision: 624775b
- id: monitor
  resource: ../../../experiments/python-fuzzing/monitor.py
  revision: 624775b
---
# Observed boundary

The combined experiment suite passed 36 tests on native macOS arm64 CPython
3.14.9 and 3.15.2, including pytest-cov branch collection. A 3.14 ctrace-core
dynamic-context run also passed. Source-filtered feedback differs between branch
directions, repeats across cases, distinguishes separate source owners, and
matches an independent interpreter in the same recorded environment.[^results]

# Ownership and identity

The monitor reserves and releases only its own tool slot and never resets global
events. It preserves another client's disabled events while allowing that
client's new-direction callbacks. Case resets clear only its own observations.
Cache keys use object identity with retained code objects, since structural
CodeType equality can merge different files. Externally visible IDs derive from
source/code/environment context, not object addresses.[^monitor]

Collected test/oracle files, framework paths, and generated pseudo filenames
are excluded. Source identification errors are reported as monitoring errors
after execution rather than injected into target behavior. Actual code roots
are disclosed in session evidence. The paired timing and its source snapshots
are recorded in the [results](../work/coverage-guided-python-fuzzing/feasibility-results.md);
they do not establish whole-engine throughput or discovery superiority.[^results][^monitor]

# Limits and recheck conditions

Coverage 7.17.0's sysmon core warns about incomplete dynamic contexts. Its own
callback rejects NUL code filenames, so the monitor's independent defense is
tested in a child without that upstream client. Do not infer that these
unsupported combined configurations work, or infer correctness from a statement
coverage percentage. Linux/Windows, Rust supervision, and guided discovery remain
unverified at this stage. Repeat the probes after changing CPython, source
filtering, ID construction, monitoring ownership, or coverage-tool versions.[^results]

[^results]: [Native probes, five self-review passes, timings, and limits](../work/coverage-guided-python-fuzzing/feasibility-results.md).
[^monitor]: [Python monitoring owner](../../../experiments/python-fuzzing/monitor.py).
