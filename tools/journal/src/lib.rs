//! The benchmark journal: record curated benchmark runs and compare them.
//!
//! Pure logic (parsing, joining, delta math, rendering) lives in library
//! modules so it is unit-testable; `main.rs` is a thin clap wiring layer.

pub mod baseline;
pub mod compare;
pub mod index;
pub mod model;
pub mod record;
