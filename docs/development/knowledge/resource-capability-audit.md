---
type: Audit
title: Resource capability experiment
description: Startup attachment, portable limits, native backend design and unverified enforcement.
status: draft
sources:
- id: results
  resource: ../work/coverage-guided-python-fuzzing/feasibility-results.md
  revision: 244ad44
- id: resource
  resource: ../../../experiments/python-fuzzing/src/resource/mod.rs
  revision: 244ad44
- id: msvc
  resource: https://learn.microsoft.com/en-us/cpp/c-runtime-library/reference/abort
  accessed: '2026-11-01'
- id: jobs
  resource: https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects
  accessed: '2026-11-01'
- id: dumpable
  resource: https://www.man7.org/linux/man-pages/man2/PR_SET_DUMPABLE.2const.html
  accessed: '2026-11-01'
- id: core
  resource: https://www.man7.org/linux/man-pages/man5/core.5.html
  accessed: '2026-11-01'
- id: native
  resource: ../work/coverage-guided-python-fuzzing/evidence/ci-native-matrix.json
  working_tree: untracked
  sha256: 5177b6bc8d78afdf7bcfc86de3c278d83a489d92fc4414d6de8fdd9d85d7d792
- id: product-resource
  resource: ../../../src/resource/mod.rs
  revision: 6d41684763c38d7c649cc32a29b8306d89229fa4
- id: product-parent
  resource: ../../../src/resource/linux.rs
  revision: 6d41684763c38d7c649cc32a29b8306d89229fa4
- id: product-hard-gate
  resource: ../../../.github/workflows/product-hard-resource.yml
  revision: 6d41684763c38d7c649cc32a29b8306d89229fa4
---
# Capability contract and observations

The experiment refuses unavailable hard caps unless best effort was explicit.
Portable fallback reports no hard memory or whole-tree containment, while ordinary
inheriting descendants are cleaned up. Native macOS CPython 3.14.9/3.15.2 each
passed 55 tests with two hard-backend skips. Native Linux/Windows enforcement and
concurrent independent allowances remain unverified. Cross-target Clippy success
is type evidence only.[^results][^resource]

A real native assignment/cleanup probe selects capabilities; each worker attaches
before the bootstrap authorizes site/target imports. Isolated bootstrap prevents
unattached PYTHONPATH startup execution. Cleanup authority is consumed before a
fallible numeric-PID operation; Lean models arbitrary repeated requests, with
bounded generated cases against the actual guard's pre-signal failure path.
Kernel calls themselves remain outside that theorem.[^results][^resource]

# Subsequent native observations

The corrected six-cell CI matrix at `7eeaa0f` passed its behavioral probes and
fixed-seed comparisons. Windows passed the aggregate/setup/concurrent hard-memory
probes using Job Objects. macOS lacks a hard backend and these Linux runners lack
a delegated cgroup, so their hard gates remain unverified. Linux's initial abort
misclassification was corrected with per-worker dumpability disabled; the actual
Linux syscall and fresh abort replay probes passed. These sampled observations
supersede the earlier cross-target-only status, not the notification or direct
Win32-output limitations.[^native]

# Classification and limits

OOM requires backend counters/notifications. Arbitrary nonzero exits cannot
establish memory enforcement. Windows suppresses CRT dialogs/report-fault; abort
can then yield exit3, which remains an ambiguous candidate alongside exception
statuses. Explicit exit(3) has the same observable status. Fresh reproduction
confirms the abnormal outcome rather than its native cause.[^results][^msvc]

The Windows backend currently relies on job-memory completion notifications.
Their delivery is not guaranteed; absence is not evidence that no limit event
occurred. Complete OOM classification remains open until native probes and
notification-limit/violation-query handling resolve that gap.[^jobs]

Linux workers additionally disable dumpability after attachment, before target
imports, because RLIMIT_CORE does not exclude piped core handlers. This changes
ptrace/process-metadata access and remains a disclosed experimental bootstrap
policy; it changes no host-wide settings.[^dumpable][^core]

Product import policy, resource backends and compatibility remain open. The probe
ignores PYTHONPATH/user-site startup configuration; it does not establish hostile
process containment. Reconsider after native runs, namespace/delegation changes,
Windows assignment/notification changes, or any altered startup sequence.[^results]

[^results]: [Observed commands, five reviews, provenance and open gates](../work/coverage-guided-python-fuzzing/feasibility-results.md).
[^resource]: [Resource/guard owner](../../../experiments/python-fuzzing/src/resource/mod.rs).
[^msvc]: [Microsoft CRT abort behavior](https://learn.microsoft.com/en-us/cpp/c-runtime-library/reference/abort).

[^jobs]: [Microsoft Job Objects notification guarantees](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects).

[^dumpable]: [Linux per-process dumpability policy](https://www.man7.org/linux/man-pages/man2/PR_SET_DUMPABLE.2const.html).

[^core]: [Linux core limits and piped handlers](https://www.man7.org/linux/man-pages/man5/core.5.html).

[^native]: [Native matrix revision, JUnit totals and observed gates](../work/coverage-guided-python-fuzzing/evidence/ci-native-matrix.json).

## Product follow-up

The product executor now attaches collection as well as execution before target
imports, and reports `oom_evidence_complete` separately from hard enforcement.
Linux exposes positive kernel counter evidence; Windows hard Job Object limits
remain distinct from completion notification delivery. Missing Windows notification
is not asserted to mean no limit event.[^product-resource] Product tests and the dedicated hard-gate
workflow are recorded in the [hard-resource review](../work/product-cli/hard-resource-review.md).

The Linux implementation can select an explicit `KIRYL_CGROUP_PARENT` only within
the resolved process cgroup ancestry/mount. A dedicated delegated parent distributes
memory to separate controller and worker leaves. kiryl does not enable controllers
on shared ancestors.[^product-parent] Native product evidence must come from actual gate execution;
the historical experiment evidence above is not substituted for those results.[^product-hard-gate]

[^product-resource]: [Product capability reporting](../../../src/resource/mod.rs).
[^product-parent]: [Delegated Linux parent](../../../src/resource/linux.rs).
[^product-hard-gate]: [Required product hard-resource gates](../../../.github/workflows/product-hard-resource.yml).
