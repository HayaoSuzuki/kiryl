# Development

This is the entry point for developers and AI agents changing kiryl.
For product usage, start with [the user guide](../user/index.md).

| Task | Start here |
| --- | --- |
| Understand decisions and implementation boundaries | [OKF catalog](knowledge/index.md) |
| Build and validate Rust changes | Setup and local checks below |
| Write, reorganize, or preview documentation | [Documentation procedure](documentation.md) |

Read relevant OKF concepts and their cited sources before implementation.
Update the owning concept and user guidance when behavior or design decisions
change. See [AGENTS.md](../../AGENTS.md) for repository documentation rules.

The development configuration follows [hoimin](https://github.com/HayaoSuzuki/hoimin):
Rust 1.99.1, edition 2024, strict Clippy checks, prek hooks, SHA-pinned GitHub
Actions, and separate vulnerability and license/source checks.

## Setup

Install Rust with rustup, then run from the repository root:

```sh
rustup toolchain install
cargo build --locked --workspace
```

`rust-toolchain.toml` selects the same compiler for local development and CI.
Commit `Cargo.lock` whenever dependencies change. The product package provides a checkout CLI with publishing disabled.
See the user guide for pinned Python adapter dependencies and execution limits.

Install [prek](https://prek.j178.dev/) and enable the local hooks:

```sh
prek install
prek run --all-files
```

Hooks run formatting and Clippy when Rust files or their configuration change.
They do not run the test suite; run that explicitly before submitting changes.

## Local checks

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --all-features --no-deps
```

Rust tests cover storage, transport, scheduler and budget behavior. Product
Python tests include actual CLI subprocess scenarios after `cargo build`.
The Python tooling tests exercise documentation validation. Run those and the
strict user-site build using the [documentation procedure](documentation.md).

For workflow checks, install uv and run:

```sh
uvx --from actionlint-py==1.7.12.25 --with shellcheck-py==0.11.0.1 actionlint
uvx zizmor==1.30.1 --no-online-audits --min-severity low .github/workflows
```

For dependency checks:

```sh
cargo install cargo-audit --version 0.22.2 --locked
cargo install cargo-deny --version 0.20.2 --locked
cargo audit --file Cargo.lock
cargo deny --workspace --all-features --locked --config deny.toml check --deny warnings licenses sources
```

`deny.toml` permits Elastic-2.0 for kiryl itself and checks third-party licenses
separately. Review the policy when adding dependencies or changing versions.

## CI and scope

CI runs on pull requests, main pushes, merge groups, and manual dispatch. It
checks Rust formatting, Clippy, tests, documentation, and workflow definitions.
The dependency workflow also runs daily. Jobs have explicit time limits and
read-only repository permissions.

Ubuntu is the initial CI environment. Passing these scaffold checks does not
establish compatibility of a future Python fuzzing engine with any platform.
Choose and validate engines during the Issue #1 experiments before adding their
dependencies or Python compatibility matrices.

The separate Documentation workflow validates OKF, repository document links,
and the user-site build, and retains the generated HTML for review.

Release automation, package publishing, and deployment are deferred. Branch
protection and repository settings are not changed by these files; configure
required checks after the workflows have run on GitHub.

## Feasibility experiments

The [guided discovery audit](knowledge/guided-discovery-audit.md) records the
isolated Rust/Python prototype, actual native observations and remaining gates.
Use the [implementation plan](work/coverage-guided-python-fuzzing/feasibility-plan.md)
and [experimental workflow](../../.github/workflows/fuzzing-feasibility.yml) for
its separate lockfiles and commands. These experiments do not install a product
CLI or establish platform support merely by configuring the matrix.

## Product adapter and CLI verification

```sh
uv sync --project python --frozen --python 3.14
cargo build --locked
uv run --project python --frozen --no-sync ruff check python
uv run --project python --frozen --no-sync ruff format --check python
uv run --project python --frozen --no-sync ty check --project python
uv run --project python --frozen --no-sync pytest -c python/pyproject.toml python/tests
lake -d formal build
formal/.lake/build/bin/scheduler-oracle --check formal/scheduler-cases.tsv
cargo run --locked --manifest-path formal/check/Cargo.toml
```

The product workflow separates the native matrix, Lean correspondence and finite
scheduler fuzz campaign. See the [acceptance record](work/product-cli/acceptance-review.md)
for local observations and unverified native hard-resource gates. Do not count
best-effort execution as hard enforcement acceptance.

## Independent cases and coverage

Follow the [test quality contract](knowledge/test-quality.md). Enumerated inputs
use rstest/pytest parameters; generated invariants use proptest/Hypothesis. One
invocation exercises one case, with no conditional or loop control flow in its
body. Keep infrastructure supervision in fixtures instead of grouping cases.

Install the pinned coverage collector and LLVM tools, then collect one native cell:

```sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --version 0.9.1 --locked
python tools/run_coverage.py --python python/.venv/bin/python
```

On Windows select `python/.venv/Scripts/python.exe`. The collector reports local
missing source; it does not claim other platforms were tested. It runs instrumented
Rust tests and the same instrumented CLI through randomized Python acceptance.
Default cargo-llvm-cov exclusions for tests are disabled. Input hashes detect
uncommitted source changes during collection; reports retain revision and dirty
state. Generated evidence lives under ignored `product-evidence/coverage/`.

Product CI collects all six native OS/interpreter cells. Download their artifacts
into one directory and run `python tools/report_coverage.py DIRECTORY`. This gate
requires all cells, identical source/revision, complete source inventory, exact
Rust line/function coverage and exact Python statement/branch coverage, including
test implementations and coverage tooling. Missing native tests and zero-hit
functions remain failures. No percentages are rounded up to meet the contract.

Linux CI uses [the dedicated delegation launcher](../../tools/with-delegated-cgroup.sh)
for hard-backend coverage and hard-resource gates. It requires the host's unified
mount to already distribute memory, provisions only its own subtree, and executes
the command as the ordinary runner. It never enables controllers on shared ancestors.
The [hard-resource workflow](../../.github/workflows/product-hard-resource.yml) owns
its native evidence rather than counting unprivileged fallback as enforcement.
