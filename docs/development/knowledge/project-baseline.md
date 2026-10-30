---
type: Contract
title: Project baseline
description: Current Rust scaffold, license, and boundaries before engine experiments.
status: draft
sources:
- id: cargo
  resource: ../../../Cargo.toml
  working_tree: untracked
  sha256: 7a48431ec2c4a231752090cb88cd26db557e08811f6f543feab5326ccd45506e
- id: toolchain
  resource: ../../../rust-toolchain.toml
  working_tree: untracked
  sha256: f715f440167e20fd8ea9afa0498c3d1ab80174cfcb8cbc895f106a34d907a1e1
- id: license
  resource: ../../../LICENSE
  working_tree: untracked
  sha256: 48255018b41fc0e965b1115af7e6779bc218bb8a6747d561da800d5022622aa2
- id: scope
  resource: ../../../AGENTS.md
  working_tree: untracked
  sha256: 13e7bdc0aa31c2f8b933edc75dd6947e091804c631597c183b99d427630db450
- id: crate
  resource: ../../../src/lib.rs
  working_tree: untracked
  sha256: 48b7ff06a659029736e92902a87e3ed732d7700ae1fd65887236570dae9b2ef2
---
# Current baseline

kiryl uses Rust edition 2024 with Rust 1.99.1 pinned for development. Workspace
Clippy settings deny `all` and `pedantic` lints. The sole package has publishing
disabled and no third-party Rust dependencies.[^cargo][^toolchain]

The repository owner selected Elastic-2.0, matching hoimin. Cargo records that
license, and LICENSE contains the license text.[^cargo][^license]

# Implementation boundary

The crate contains development scaffolding, with no fuzzing implementation.
Rust is the intended primary implementation language. Engine selection remains
open: Atheris is a candidate, rather than an installed dependency or an
established architectural requirement. Publishing and deployment are deferred.[^crate][^scope]

# Recheck conditions

Update this baseline when the compiler, license, package layout, engine choice,
or publishing policy changes. Add independently retrievable decisions for
engine evaluation and execution contracts as they become concrete. Do not infer
runtime guarantees from this scaffold or from a successful empty test suite.[^cargo][^crate]

[^cargo]: [Cargo metadata and lint policy](../../../Cargo.toml).
[^toolchain]: [Pinned Rust toolchain](../../../rust-toolchain.toml).
[^license]: [Elastic License 2.0](../../../LICENSE).
[^scope]: [Project scope](../../../AGENTS.md).
[^crate]: [Initial library](../../../src/lib.rs).
