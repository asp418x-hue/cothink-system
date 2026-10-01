//! Lint-policy anchor for the integration-test directory.
//!
//! By default `cargo clippy` lints only the library and the binaries; the
//! per-file crates under `tests/` are swept only when asked for with
//! `cargo clippy --all-targets` (or `--tests`). Because the harness
//! compiles this crate with `cfg(test)` set, the gate below keeps it empty
//! for both `cargo test` and clippy-on-tests, so it contributes no lint
//! surface. The auto-extracted Dart reference suites carry their own
//! per-file `#![allow(non_snake_case)]` since their names mirror the Dart
//! originals.
#![cfg(not(test))]
