---
type: Audit
title: Finite pytest invocation lifecycle experiment
description: Limited native Python observations and Lean-model correspondence for normal fixture returns.
status: draft
sources:
- id: results
  resource: ../work/coverage-guided-python-fuzzing/feasibility-results.md
  working_tree: untracked
  sha256: 2a0196ed7d2cafff3c55365638a67c87c5f09d33ea4dc94860ae59e7f75a9144
- id: model
  resource: ../../../experiments/python-fuzzing/formal/ScopeModel.lean
  working_tree: untracked
  sha256: 6c03aaef4d438f4bc6efcbc92debf73a5f836e40d4564644fece05068b982c87
---
# Observed boundary

The experimental adapter passed 11 tests on native macOS arm64 with CPython
3.14.9 and 3.15.2, Hypothesis 6.170.13, and pytest 9.2.0. The observations cover
ordinary synchronous properties, explicit fixture/parameter/self binding,
function resets between finite invocations, shared module/session scopes,
selection/error classification, generated schedules, and model correspondence.
These are not integrated Rust-controller or production compatibility results.
Linux/Windows and the later feasibility gates remain unrun.[^results]

# Formal claim and sensitivity

Lean proves that the model's N normal invocations have exactly N starts and N
finishes, for arbitrary natural N. The model excludes abnormal termination,
exceptions, and arbitrary interleavings. Fixed broken witnesses distinguish
missing finish, duplicate finish, and the zero/one boundary. This theorem does
not prove pytest or Python implementation correctness.[^model]

A dedicated executable generates the expected-count corpus for N = 0..8.
The real adapter's fixture observations matched those nine same-premise cases.
The [results record](../work/coverage-guided-python-fuzzing/feasibility-results.md)
contains the correspondence modes, safety bounds, five self-review passes,
source hashes, reproduction commands, and the targeted one-mutant sensitivity
result. Ruff ALL, formatting, and ty all-error checks passed; exceptions are
narrow and recorded in experiment configuration.[^results]

# Recheck conditions

Repeat the adapter and corpus checks when invocation construction, parameter
fixture metadata, method-instance binding, pytest/Hypothesis versions, or model
transitions change. Extend the model explicitly before claiming cancellation,
failure, or process-isolation guarantees. Never infer them from balanced normal
returns or carry these observations to an untested platform.[^results][^model]

[^results]: [Feasibility observations and limits](../work/coverage-guided-python-fuzzing/feasibility-results.md).
[^model]: [Lean fixture count model](../../../experiments/python-fuzzing/formal/ScopeModel.lean).
