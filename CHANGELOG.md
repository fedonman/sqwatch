# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Set up the CI/CD infrastructure for the project. The CI pipeline runs on every pull request and push to main, checking formatting with `cargo fmt`, linting with `cargo clippy`, running the test suite, validating against the minimum supported Rust version (1.85.0), and auditing dependencies for license compliance and known vulnerabilities via `cargo-deny`. Every pull request is also required to include a changelog entry. A separate release workflow, triggered manually, validates the version, publishes the crate to crates.io, and creates a GitHub Release with the changelog section as release notes. [PR #1](https://github.com/fedonman/sqwatch/pull/1)

### Changed

- Overhauled the filter pane and titlebar layout. The titlebar is now split into four distinct sections: brand, logged-in user, active filters, and a dedicated flash notification bar that takes the remaining space. Filter and column settings are now persisted to disk (`~/.config/sqwatch/`) and restored on launch. The user filter was changed from exact match to regex-based matching, and node filtering was converted from a free-text regex field to a selectable list populated from `sinfo`. The squeue query now passes `--all` and `--states=all` by default so that all job states are displayed, including newly recognized states `SUSPENDED` and `OUT_OF_MEMORY`. Job cancellation via `scancel` now properly checks exit status and reports errors through the flash bar instead of silently discarding failures. Job selection and cursor position are preserved across the 1-second auto-refresh cycle by remapping via job ID. The statusbar was reorganized with keybinding hints on the left and right-aligned job statistics, and several keybindings were updated: script inspection moved from `Enter` to `s`, the `q` close binding was removed from overlay panes in favor of `Esc`, and `PageUp`/`PageDown`/`Ctrl+U`/`Ctrl+D` were removed from the script pane. Column headers no longer change color when used for sorting. [PR #2](https://github.com/fedonman/sqwatch/pull/2)
