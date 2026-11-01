---
type: Audit
title: Guided discovery feasibility evidence
description: Local fixed-budget discovery, isolated baselines, model correspondence and native gates.
status: draft
sources:
- id: results
  resource: ../work/coverage-guided-python-fuzzing/feasibility-results.md
  working_tree: modified
  sha256: 366ee328f903b7a0a49c7af3f69aeaf1df56539c2a899f31244ecd4871428bac
- id: search
  resource: ../../../experiments/python-fuzzing/src/search.rs
  working_tree: untracked
  sha256: a5746c77acfc291411324d20efa10cd0205e992b2b11e85a6ad2712eec22f3e2
- id: evidence
  resource: ../../../experiments/python-fuzzing/evidence.py
  working_tree: untracked
  sha256: 84128ceafe6b097a71e3c2172f7711c8ea592bd3f833d670a17f99ab6ca9e51d
- id: sample314
  resource: ../work/coverage-guided-python-fuzzing/evidence/macos-python314.json
  working_tree: untracked
  sha256: a4ffc5d44f91c54a5fee299717321f5ab7344b7166d8862596e960e1e840a05a
- id: sample315
  resource: ../work/coverage-guided-python-fuzzing/evidence/macos-python315.json
  working_tree: untracked
  sha256: 321f11a309121b795aced57fa3b335ed3e911a4ef78f4f8b191e2d5a6c24e78b
---
# Observed discovery

Native macOS CPython 3.14.9/3.15.2 each completed five seeds, two authored targets
and three modes with 20-second limits. Guided search discovered and freshly
confirmed both planted defects in every seed. Mutation without feedback found the
stateful defect but not the nested branching defect; ordinary Hypothesis found
both. These synthetic samples demonstrate retention driven by application branches
and reproducible discoveries, not general performance superiority.[^results][^sample314][^sample315]

# Owners and evidence integrity

Rust owns seeded bounded byte mutation, novelty retention, time/process control
and original/derived persistence. Python owns pinned pytest/Hypothesis execution,
monitoring and opaque semantic replay. Rejects do not contribute novelty; only
valid passing inputs canonicalize retained buffers. The bounded model/count
correspondence and finite proptest/fuzz observations have explicit premises;
none proves the whole implementation.[^search][^results]

Every comparison starts with separate fresh database/corpus parents. Evidence
validation checks native OS/minor provenance, source/report hashes, measurement
mode, seed, storage separation, complete probe categories and matching JUnit
interpreter metadata. It rejects unrun platform completion claims. Structural
integrity is distinct from factual review. Compact summaries retain observed
metrics/hashes; raw replay bundles are local generated output or seven-day CI
artifacts. Attempt counts have different measurement scopes across modes.[^evidence][^results]

# Decision and limits

Continue architecture A's native experiments; keep production engine adoption
open. Local fallback behavior and 68 passing tests per interpreter do not establish
hard memory enforcement or native Linux/Windows support. Cross-target checks and
a defined workflow are not native execution. Full campaign/CLI scope remains the
plan's dependent follow-on. Reconsider after actual native probes, strategy/pin,
coverage ID, transport/resource, corpus or sampling-method changes.[^results]

[^results]: [Commands, five reviews, scope and remaining gates](../work/coverage-guided-python-fuzzing/feasibility-results.md).
[^search]: [Rust mutation/retention owner](../../../experiments/python-fuzzing/src/search.rs).
[^evidence]: [Evidence integrity validator](../../../experiments/python-fuzzing/evidence.py).
[^sample314]: [CPython 3.14 compact observations](../work/coverage-guided-python-fuzzing/evidence/macos-python314.json).
[^sample315]: [CPython 3.15 compact observations](../work/coverage-guided-python-fuzzing/evidence/macos-python315.json).
