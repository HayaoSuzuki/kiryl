# kiryl user guide

kiryl explores existing synchronous Hypothesis tests through a local Rust CLI
and a separate Python interpreter. It is available from a source checkout;
package publication is deferred. Tests are trusted local code, not sandboxed code.

## Prepare a checkout

Install the Rust toolchain selected by `rust-toolchain.toml` and uv, then run:

```sh
cargo build --locked
uv sync --project python --frozen --python 3.14
```

The adapter supports GIL-enabled CPython 3.14/3.15 with Hypothesis 6.170.13 and
pytest 9.2.0. Version checks reject other runtimes before test dispatch. Install
your project's dependencies in the selected interpreter as well. The native
product workflow tests the OS/interpreter matrix; a configured job alone is not
proof of support. Recorded native hard-resource probes passed on Linux and Windows with both
specified Python versions, including aggregate descendants and cleanup. The
product CI matrix records actual CLI scenario results; consult the PR checks for
the tested revision rather than treating this as unrestricted platform support.

## Explore tests

From your test project, run the built executable with its absolute path:

```sh
/path/to/kiryl/target/debug/kiryl run tests/ --python /path/to/python \
  --duration 60 --store .kiryl --report report.json --best-effort
```

On Windows use `kiryl.exe`. Paths, node IDs, `-k` expressions and `-m` markers
select pytest tests. The adapter supports synchronous `@given` tests, fixture
arguments, parameter variants and Hypothesis state machines. Fixed-input tests
are excluded. No new input language or rewritten assertion is required.

Omit `--duration` to continue until interruption. `--case-timeout` (default 10
seconds) starts after fixture setup. `--slice` (default 1 second) controls
cooperative allocation: a running case can finish before pytest teardown.
Targets receive exploration time rather than equal numbers of examples; overrun
is charged against their next allocation. Each worker is a real pytest session,
so module/session fixtures belong to that worker's session and function fixtures
are refreshed at invocation boundaries.

The campaign budget includes collection, setup, search, replay and shrinking.
Process cleanup and final persistence can continue after the budget. A candidate
active at interruption or expiry is retained as unverified, not reported as a
passing test. Short budgets can leave selected targets unexecuted in the report.

## Limits and output

By default kiryl requires a hard memory limit for the worker process tree.
`--memory-mib` defaults to 1024. Linux requires a writable delegated cgroup;
Windows uses a Job Object. For delegated Linux layouts, `KIRYL_CGROUP_PARENT`
selects an existing ancestor within the process's cgroup-v2 mount. The delegated
parent must already distribute the memory controller; run kiryl in its separate
controller leaf so workers can occupy sibling groups. kiryl never enables shared
ancestor controllers automatically. On macOS hard tree enforcement is unavailable.
`--best-effort` explicitly accepts weaker enforcement; the report states actual
capabilities. These process limits are not a security sandbox. Arbitrary abnormal
termination is not proof of OOM, especially when Windows notifications are absent.

`--output-bytes` defaults to 65536 retained bytes across stdout and stderr. kiryl
keeps the tail and continues draining excess output. Raw FD output and binary
output do not share the control protocol. Output truncation alone is not a test
failure. `--seed` selects deterministic mutation randomness; timing and external
state can still change results.

## Resume, replay and minimize

Repeat the same run command and store path to seed a new campaign from the saved
bounded corpus. This does not restore fixture state or Python execution position.
The manifest rejects changed source/configuration, parameter target identity,
detected interpreter/dependency identity, or incompatible storage schemas without
deleting data. External services, native libraries and unrecorded environment
state remain reproduction assumptions. Keep durable regression tests yourself.

The JSON report identifies finding files. Originals contain exact input bytes,
available rendered values, fingerprint and observation. Preserve these files;
verification and minimization write separate derivatives.

```sh
/path/to/kiryl/target/debug/kiryl replay .kiryl/target-0/finding-0.bin \
  --python /path/to/python --duration 20 --best-effort
/path/to/kiryl/target/debug/kiryl minimize .kiryl/target-0/finding-0.bin \
  --python /path/to/python --duration 20 --best-effort
```

Replay uses a fresh worker. Semantic minimization requires a Hypothesis replay
token; raw native crashes/timeouts can be replayed but cannot always be
semantically minimized. A candidate is confirmed only by fresh verification of
its failure fingerprint. Pending, unstable and skipped observations remain
separate from confirmation; original files are never replaced by derivatives.

Run the original test normally with `python -m pytest NODEID` to use ordinary
Hypothesis checks, its database and shrinking. Fuzz mode uses its own corpus and
budgets; Hypothesis `max_examples`, phases and deadline are not campaign limits.
A database disabled in normal pytest does not replay kiryl artifacts; use the
explicit replay command. Opaque tokens depend on the pinned strategy/runtime and
are not portable long-term regression test data.

Exit 0 means no finding in the executed search, not correctness. Findings and
unresolved candidates return 1, infrastructure/configuration failures return 2,
and user interruption returns 130. Inspect per-target status and verification
fields rather than interpreting every nonzero result as a confirmed test defect.

## Storage failures

A writer lock prevents concurrent campaigns. If a process dies, `writer.lock`
can remain. Confirm that no campaign owns the store before manually removing a
stale lock; kiryl never automatically removes it. Keep originals when repairing
space/permission failures. Machine-crash durability and automatic migration of
old formats are not promised.

kiryl requires no account or outbound connection. Your tests may communicate
with their own services. kiryl uses the [Elastic License 2.0](https://github.com/HayaoSuzuki/kiryl/blob/main/LICENSE).
