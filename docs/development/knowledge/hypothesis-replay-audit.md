---
type: Audit
title: Hypothesis stateful replay and semantic minimization experiment
description: Native interpreter observations for database identity, fresh replay, and bounded semantic shrinking.
status: draft
sources:
- id: results
  resource: ../work/coverage-guided-python-fuzzing/feasibility-results.md
  revision: a36750985a30bee80e1920dfe3279f06fae45bac
- id: bridge
  resource: ../../../experiments/python-fuzzing/hypothesis_bridge.py
  revision: a36750985a30bee80e1920dfe3279f06fae45bac
- id: corrections
  resource: ../work/coverage-guided-python-fuzzing/feasibility-results.md
  working_tree: modified
  sha256: 157f9579e23f6de74254248a9ca3f6ec34e190e67a3b83d55e9ce0adabb746e9
---
# Observed contract slice

The experimental adapter's combined 23-test suite passed on native macOS arm64
with CPython 3.14.9 and 3.15.2. Ordinary and stateful planted failures replayed
in fresh interpreters and produced semantically smaller witnesses. Ordinary
pytest replay was tested with generation disabled (`Phase.reuse` only), including
parameter-variant database isolation. Database-disabled tests still supported
explicit replay and reported automatic database replay unavailable.[^results]

# Ownership and failure semantics

The exact-version bridge owns raw/choice token representations, environment
validation, Hypothesis database writes, and semantic shrinking. It acknowledges
replayable candidate tokens before execution, checks budget after acknowledgment,
retains the immutable original, and selects the shrinker's best result rather
than its last attempted failure. Changed failure identity cannot become the
minimization target. Successful replay of an earlier failure is unstable rather
than confirmed.[^bridge][^results]


The subsequent whole-branch review corrected generated-input raw replay,
post-setup case timing and explicit skip propagation. These corrections and
regression observations supersede the earlier implementation on those points;
original comparison snapshots retain their historical scope.[^corrections]

# Limits and recheck conditions

These are sampled integration results. They do not prove arbitrary custom
strategies, native-crash-safe shrink execution, OS resource enforcement, bounded
subprocess logs, or Linux/Windows compatibility. Opaque strategy inversion values
without a safe token encoding fail explicitly; the tested cases did not need
that representation. No formal theorem about Hypothesis's shrinker is claimed.
Repeat the observations when the pinned Hypothesis version, bridge, database
identity, candidate encoding, or budget/verification rules change.[^bridge][^results]

[^results]: [Feasibility observations, five review passes, and source snapshots](../work/coverage-guided-python-fuzzing/feasibility-results.md).
[^bridge]: [Exact-version Hypothesis integration](../../../experiments/python-fuzzing/hypothesis_bridge.py).

[^corrections]: [Whole-branch review corrections](../work/coverage-guided-python-fuzzing/feasibility-results.md).
