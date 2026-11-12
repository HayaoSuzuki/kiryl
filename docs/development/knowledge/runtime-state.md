---
type: Contract
title: Typed runtime states and stable serialization
description: Internal enum boundaries, unknown label retention and diagnostic separation.
status: draft
sources:
- id: design
  resource: ../work/product-cli/typed-state-design.md
  working_tree: untracked
  sha256: 6048f402172e836a6e668c69adf3cb12ae33f92ce56feffdace1c630319315cb
- id: product
  resource: ../../../src/executor.rs
  working_tree: modified
  sha256: 87803b8cf069e52007c819e7fdcfbf39de98f17765d63e329eec64802058aa21
- id: verification
  resource: ../../../src/verification.rs
  working_tree: modified
  sha256: c3530e563d93ae709f4b0b98129b86ab8a7e28cf3bb0eb6e938820108d4727e6
- id: python
  resource: ../../../python/kiryl_adapter/model.py
  working_tree: untracked
  sha256: dce97fb2d51e9ae2f78609d11e2b3ea3bc399497bedc30806905e6728635b86a
- id: experiment
  resource: ../../../experiments/python-fuzzing/src/types.rs
  working_tree: untracked
  sha256: 93e5be05b4196a7269f77393374b315721234d0ffc33ea0c582801824c9fe36b
- id: experiment-python
  resource: ../../../experiments/python-fuzzing/model.py
  working_tree: untracked
  sha256: 92ed9ef16bef50056352888e46fd9bfe120c9cd842dd2c0c4984dcc0ae19ef3e
---
# State ownership

Internal producers and consumers exchange enums or domain types. String labels
belong to CLI, JSON, saved-record, pytest and evidence boundaries. Identifiers,
paths, fingerprints and human diagnostics are unbounded data, not enumerations.
The product and standalone experiment retain separate model modules and do not
import one another.[^design][^product][^python][^experiment][^experiment-python]

Semantic operations carry their required opaque token. Empty tokens are valid;
absence is a different operation or an admission error. Start encoding owns wire
mode aliases; report modes remain independent. Incoming control discriminators
must be strings, even where serde accepts externally tagged enum objects.
Typed capabilities stay structured until report serialization.[^product]

Verification has a closed verdict and explicit empty/diagnostic representations.
Unknown version-1 result labels preserve their exact identity in an Other variant
or an opaque typed wrapper; they do not become successful execution or failures
by default. Benchmark failure selection ignores unknown labels as before. Saved
missing values keep their original empty-string defaults. Python JSON ingress
restores actual enum values, and human presentation encodes labels explicitly
rather than exposing Enum repr.[^verification][^python][^experiment][^experiment-python]

# Validation and reconsideration

The [design and verification record](../work/product-cli/typed-state-design.md)
records alternatives, five self-reviews per phase, independent findings and fixes.
Literal serialization, unknown/empty/non-string cases, arbitrary label/token
properties and existing native tests protect compatibility. Generated Lean oracle
rows remain actual testcase inputs, with one row per invocation.[^design]

Revisit the model when an operation needs a new payload or a genuinely new state
appears. Update enum, ingress, policy, serialization and contract tests together;
do not add string-equality shims or independent nullable state fields. Preserve
unknown-label behavior where the versioned format is extensible. Native exact
coverage is a separate measured requirement and cannot be inferred from type
checking or an earlier revision's report.[^design]

[^design]: [Typed state exchanges](../work/product-cli/typed-state-design.md).
[^product]: [Product execution states](../../../src/executor.rs).
[^verification]: [Verification states and diagnostics](../../../src/verification.rs).
[^python]: [Product Python states](../../../python/kiryl_adapter/model.py).
[^experiment]: [Isolated Rust experiment states](../../../experiments/python-fuzzing/src/types.rs).
[^experiment-python]: [Isolated Python experiment states](../../../experiments/python-fuzzing/model.py).
