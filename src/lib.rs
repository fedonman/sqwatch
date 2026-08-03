//! sqwatch — a terminal UI for watching and managing SLURM job queues.
//!
//! The binary in `main.rs` is a thin wrapper around [`dashboard::Dashboard`];
//! the modules are re-exported here so the logic can be exercised by tests.

pub mod backend;
pub mod core;
pub mod dashboard;
pub mod views;
