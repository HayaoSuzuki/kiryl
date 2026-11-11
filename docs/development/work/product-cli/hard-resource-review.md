# Product hard-resource gate review

## Change and scope

The product Linux backend can use `KIRYL_CGROUP_PARENT` to select a provisioned
cgroup-v2 parent. The path is canonicalized, must remain within a resolved
cgroup-v2 mount, and must contain this controller's current membership. A sibling
or unrelated directory is rejected. Without the variable, current-membership
selection remains unchanged. The backend creates and configures only its owned
worker groups; it never enables controllers on shared ancestors.

This resolves the occupied-leaf problem: a delegated parent can distribute
`memory` while the controller occupies its own leaf and workers occupy sibling
leaves. The controller's occupied leaf need not distribute controllers. The
[kernel cgroup-v2 documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html)
defines these controller and process-placement rules.

The [required workflow](../../../../.github/workflows/product-hard-resource.yml)
contains separate Linux and Windows jobs for CPython 3.14 and 3.15. Linux requires
an existing unified mount distributing memory, provisions an isolated subtree
with sudo, enables memory only within that subtree, and puts the test controller
in a separate leaf. Where supported, this owned subtree disables swapping so
resident-memory pressure cannot be hidden by swap. Missing prerequisites fail
the dedicated job; they do not become successful skips. The gate validates
privileged CI provisioning, not rootless systemd delegation on arbitrary hosts.

The [evidence runner](evidence/hard-resource-run.py) invokes the actual product
CLI without `--best-effort`. A 768 MiB control requires two descendants to retain
192 MiB each. The 256 MiB run must exercise the hard backend under aggregate
pressure. Allocation touches each 4 KiB interval so lazy zero-filled pages do not
invalidate the resident-memory probe. A separate sleeping descendant is left to
the supervisor, and recorded descendant PIDs must be terminated afterward.
Linux additionally requires an OOM outcome linked to an immutable original and
removal of all owned worker cgroups. Reports, originals, stderr, and process
observations are retained as workflow artifacts, including on gate failure.

Windows requires Job Object capabilities. It accepts positive OOM notification,
recorded allocation refusal with an assertion finding, or an abnormal native
candidate. The last case does not establish the cause of termination. Its
classification artifact explicitly records incomplete OOM classification;
missing notification never establishes absence of a limit event. Microsoft
[documents the notification limitation](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects).
The gate does not repair that limitation or claim reliable causal classification.

## Review passes

1. Parent selection: canonicalization precedes containment checks; component-wise
   ancestry prevents sibling/prefix confusion. Owned groups alone are modified.
2. CI topology: controller and worker leaves are siblings; shared ancestors are
   checked but never enabled or reconfigured. Cleanup removes only owned groups.
3. Probe sensitivity: added an ample-memory control, explicit page touching,
   aggregate descendants, and an intentionally lingering child. A passing smoke
   campaign cannot satisfy this hard gate.
4. Evidence: immutable originals are stored outside temporary project cleanup;
   reports and process observations remain available after failures. Windows
   ambiguous candidates retain their stated limits.
5. Claims: local checks below cover parsing, validation and cross-target typing.
   Actual Linux/Windows enforcement, configured memory thresholds and cleanup
   timing remain unexecuted until this workflow runs. The gate does not cover
   independent concurrent campaign allowances or hostile process escape.

## Local verification

- Added the parent-selection regression before implementation; standalone Rust
  test compilation initially failed because the validation function was absent.
- Standalone `rustc --test` execution passed the parent-validation regression.
  This runs path validation on macOS and is not Linux enforcement evidence.
- Linux-target library Clippy passed with warnings denied.
- Cached actionlint 1.7.12.25 with ShellCheck 0.11.0.1 accepted the workflow.
- Ruff check/format and Python syntax checks accepted the evidence runner;
  generated target and child source were also parsed explicitly.
- `git diff --check` passed.

The first native CI run must supply the resource evidence. Configuring jobs and
cross-compiling code do not establish native product compatibility. The existing
resource audit owner should link successful or failed observations with their
revision before changing its acceptance claims.

## First native gate failures and corrections

Run `43008964697` at merge revision `da8d9a1` did not establish hard-resource
acceptance. Linux failed during collection before executing the control. The
runner canonicalized the requested interpreter path with `resolve()`, following
Linux virtualenv symlinks into the base interpreter and losing the pinned
virtualenv dependencies. A local regression reproduced this path change before
switching to `absolute()`, which preserves the selected virtualenv symlink.
The sudo scope now also preserves only `LD_LIBRARY_PATH` for dynamically linked
hosted Python installations. Collection diagnostics are handled separately.

Windows completed 48 cases at the initial 256 MiB allowance: all recorded pairs
of 96 MiB allocations succeeded. That workload was insufficient to demonstrate
the intended aggregate denial. Each descendant now commits and touches 192 MiB,
so their combined allocations alone exceed the 256 MiB Job Object allowance;
the ample-memory control is increased to 768 MiB. This changes the workload,
not the enforcement assertion.

The Windows cleanup failure used numeric PIDs across many worker lifetimes;
recorded PIDs were reused in different child records. It therefore did not
establish that the original process survived. Before allocation is authorized,
the target now duplicates each actual CPython `Popen` process handle into the
external evidence observer. Cleanup checks query those owned handles and close
them afterward. A live original descendant still fails the gate, while PID reuse
cannot impersonate it. This is a CI evidence mechanism, not a product API or
change to Windows cleanup implementation. The next native run must determine
whether the exact descendants terminate and whether aggregate denial occurs.
