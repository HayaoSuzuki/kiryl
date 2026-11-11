//! Fuzz testing tools for Python.
//!
//! Single-worker campaigns use a separate Python adapter and immutable finding records.
//! Native hard-resource acceptance is platform-dependent; inspect reported capabilities.

pub mod artifacts;

pub mod campaign;
pub mod executor;
mod resource;
mod search;
mod wire;
