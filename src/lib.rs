//! sqwatch is a terminal UI for watching and managing SLURM job queues.
//!
//! The binary in `main.rs` is a thin wrapper around [`dashboard::Dashboard`];
//! the modules are re-exported here so the logic can be exercised by tests.
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/fedonman/sqwatch/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/fedonman/sqwatch/main/assets/logo.png"
)]

pub mod backend;
pub mod core;
pub mod dashboard;
pub mod views;
