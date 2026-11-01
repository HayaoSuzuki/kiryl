---
type: Audit
title: Rust supervision experiment
description: Native process recovery, bounded transport, persistence and budget evidence.
status: draft
sources:
- id: results
  resource: ../work/coverage-guided-python-fuzzing/feasibility-results.md
  revision: 962198c
- id: controller
  resource: ../../../experiments/python-fuzzing/src/controller.rs
  revision: 962198c
- id: corrections
  resource: ../work/coverage-guided-python-fuzzing/feasibility-results.md
  working_tree: modified
  sha256: ac186e1b4b844672fe5a1f9c11ae93d1e74a2c121094953c1701157bde6074f6
---
# Observed behavior

The isolated Rust parent retains active raw inputs, writes originals before
acknowledging failures, runs fresh replay, and acknowledges opaque shrink tokens.
Real native crashes and bounded binary floods preserve evidence and independent
tests continue. Native macOS CPython 3.14.9/3.15.2 each pass 48 experiment tests;
Rust's eight tests include framing, log-tail, artifact proptest and a Lean-generated
budget corpus. Bounded cargo-fuzz runs and exact source snapshots are recorded in
the evidence.[^results][^controller]

# Ownership and limits

Python owns pytest/Hypothesis semantics; Rust owns control framing, active input,
time, original/derived writes and logs. Campaign expiry precedes a case timeout;
interrupts and transport failures remain distinct. Derived evidence never replaces
original bytes. Abstract Lean theorems cover budget precedence and append-only
finding state, not Python, operating-system behavior or filesystem durability.
The corpus checks the live Rust predicate helper with injected predicates, while
real process tests check timing and termination separately.[^results][^controller]

These are finite experiments with explicit best-effort opt-in, fresh workers per
invocation, and pending verification on returned best shrink tokens. They do not
establish hard memory enforcement, hostile-process containment, production
scheduling, discovery effectiveness, or native Windows/Linux support. Recheck when
transport, budget boundaries, persistence, process lifetime or the bridge changes.
Resource control and streaming search require their own subsequent evidence.[^results]


The subsequent whole-branch review corrected generated-input raw replay,
post-setup case timing and explicit skip propagation. These corrections and
regression observations supersede the earlier implementation on those points;
original comparison snapshots retain their historical scope.[^corrections]

[^results]: [Observed commands, five reviews, exact snapshots and limits](../work/coverage-guided-python-fuzzing/feasibility-results.md).
[^controller]: [Experimental supervision owner](../../../experiments/python-fuzzing/src/controller.rs).

[^corrections]: [Whole-branch review corrections](../work/coverage-guided-python-fuzzing/feasibility-results.md).
