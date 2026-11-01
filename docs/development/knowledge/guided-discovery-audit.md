---
type: Audit
title: Guided discovery feasibility evidence
description: Local fixed-budget discovery, isolated baselines, model correspondence and native gates.
status: draft
sources:
- id: results
  resource: ../work/coverage-guided-python-fuzzing/feasibility-results.md
  working_tree: modified
  sha256: ac186e1b4b844672fe5a1f9c11ae93d1e74a2c121094953c1701157bde6074f6
- id: search
  resource: ../../../experiments/python-fuzzing/src/search.rs
  revision: bda21333dcef690dc53df8d318763eeb9bd5c78c
- id: evidence
  resource: ../../../experiments/python-fuzzing/evidence.py
  working_tree: modified
  sha256: e586e11c46c2d46f5e23e6898e5204f9e5d60029a81249fee08cfbf103034084
- id: sample314
  resource: ../work/coverage-guided-python-fuzzing/evidence/macos-python314.json
  revision: bda21333dcef690dc53df8d318763eeb9bd5c78c
- id: sample315
  resource: ../work/coverage-guided-python-fuzzing/evidence/macos-python315.json
  revision: bda21333dcef690dc53df8d318763eeb9bd5c78c
- id: native
  resource: ../work/coverage-guided-python-fuzzing/evidence/ci-native-matrix.json
  working_tree: untracked
  sha256: 5177b6bc8d78afdf7bcfc86de3c278d83a489d92fc4414d6de8fdd9d85d7d792
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
mode, seed, storage separation, complete probe categories, matching JUnit
interpreter metadata and agreement between declared probe statuses and every
required testcase outcome. Missing/skipped cases remain unverified. It rejects unrun platform completion claims. Structural
integrity is distinct from factual review. Compact summaries retain observed
metrics/hashes; raw replay bundles are local generated output or seven-day CI
artifacts. Attempt counts have different measurement scopes across modes.[^evidence][^results]

# Decision and limits

Continue architecture A's native experiments; keep production engine adoption
open. The subsequent native matrix passed all six OS/minor jobs and 180
comparisons. Windows passed the tested hard-memory gates; macOS and these Linux
runners retain unverified hard-memory gates. Synthetic native observations do not
establish product platform support or complete Windows notification/Win32-output
semantics.[^native] Full campaign/CLI scope remains the
plan's dependent follow-on. Reconsider after actual native probes, strategy/pin,
coverage ID, transport/resource, corpus or sampling-method changes.[^results]

[^results]: [Commands, five reviews, scope and remaining gates](../work/coverage-guided-python-fuzzing/feasibility-results.md).
[^search]: [Rust mutation/retention owner](../../../experiments/python-fuzzing/src/search.rs).
[^evidence]: [Evidence integrity validator](../../../experiments/python-fuzzing/evidence.py).
[^sample314]: [CPython 3.14 compact observations](../work/coverage-guided-python-fuzzing/evidence/macos-python314.json).
[^sample315]: [CPython 3.15 compact observations](../work/coverage-guided-python-fuzzing/evidence/macos-python315.json).

[^native]: [Corrected native CI manifest and compact observations](../work/coverage-guided-python-fuzzing/evidence/ci-native-matrix.json).
