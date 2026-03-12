# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Set up the CI/CD infrastructure for the project. The CI pipeline runs on every pull request and push to main, checking formatting with `cargo fmt`, linting with `cargo clippy`, running the test suite, validating against the minimum supported Rust version (1.85.0), and auditing dependencies for license compliance and known vulnerabilities via `cargo-deny`. Every pull request is also required to include a changelog entry. A separate release workflow, triggered manually, validates the version, publishes the crate to crates.io, and creates a GitHub Release with the changelog section as release notes. [PR #1](https://github.com/fedonman/sqwatch/pull/1)
