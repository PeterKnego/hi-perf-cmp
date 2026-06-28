//! Benchmark-only comparison of our hand-rolled SPSC/MPSC handoff rings against
//! the `disruptor` crate (and crossbeam / std-mpsc references). NOT part of the
//! cross-language result-contract grid; the library is std-only.

pub mod mpsc;
pub mod spsc;
