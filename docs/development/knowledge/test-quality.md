---
type: Contract
title: Independent test cases and exact native coverage
description: Test granularity, generated properties, strict Python checks and native coverage gates.
status: draft
sources:
- id: plan
  resource: ../work/product-cli/test-coverage-plan.md
  working_tree: modified
  sha256: 068641eb3754d111a4c74d2062ea81f5c02ce9064ae7df1d9bf510c737380420
- id: manifest
  resource: ../../../Cargo.toml
  revision: 0cfe0b2406db2f4cb0a0457bd663a736bbdb4970
- id: mock-api
  resource: https://docs.python.org/ja/3.15/library/unittest.mock.html#unittest.mock.create_autospec
---
# Test contract

The maintainer requires one scenario or generated property per test invocation.
Test functions contain no conditional/loop control flow. Use independent rstest
or pytest parameters for enumerated inputs, and proptest/Hypothesis when the
behavior has a useful general invariant. Infrastructure helpers may supervise a
single scenario; moving unrelated testcase iteration into helpers is not compliance.
Assertions may verify multiple consequences of that same scenario.[^plan][^manifest]

Do not use pytest monkeypatch fixtures or MonkeyPatch objects. Callable test doubles
use create_autospec against the real function, class or instance, with spec_set when
its attributes are available. Scoped patch.object/patch.dict contexts restore names
and constants. Autospeccing checks supported attributes and call signatures; it does
not replace behavior assertions. Foreign APIs absent on the host use a narrow
documented interface, not unrestricted mocks. Runtime measurement interception keeps
the real measured factory rather than introducing test doubles into execution.[^mock-api]

Python uses Ruff ALL and ty rules at error severity with strict equality/generic
narrowing and no ignored typing diagnostics. pytest-cov measures statements and
branches; pytest-randomly exposes ordering dependencies. Product modules, actual
test implementations and the coverage tools stay in measurement scope.[^plan]

Rust uses cargo-llvm-cov, including inline test modules and integration test files.
Disable the tool's default test-directory exclusion. Instrument the CLI invoked
by Python acceptance tests too. Keep zero-hit functions and report unexecuted
source locations rather than rounding a value to100%. Native reports must identify
the same source revision and include Linux, macOS and Windows. Combining native
execution is necessary because platform-specific tests must remain visible.[^plan]

Python statements/branches must be exactly100%. Rust lines/functions target100%;
only a genuinely uninducible path may have a narrow inline exclusion with a concrete
reason. Do not exclude modules, tests or ordinary reachable error paths to raise a
score. Branch/region/instantiation measures are distinct from line coverage and
must not be claimed when only lines were measured.[^plan]

# Evidence and reconsideration

The [test and coverage work record](../work/product-cli/test-coverage-plan.md)
retains the design, five reviews per phase and measured scope. Current exact native
results belong to the tested revision's PR checks; a configured gate is not proof
that it passed. Revisit measurement when adapters, target platforms, compiler,
coverage instrumentation, test locations or generated fixtures change.[^plan]

[^plan]: [Test cases and coverage plan](../work/product-cli/test-coverage-plan.md).
[^manifest]: [Rust development dependencies](../../../Cargo.toml).

[^mock-api]: [Python create_autospec and scoped patching](https://docs.python.org/ja/3.15/library/unittest.mock.html#unittest.mock.create_autospec).
