---
applyTo: '**/*.rs'
---
# Rust Code Style Guide
* Use `cargo fmt` to format your code.
* Use `cargo clippy` to lint your code.
* Use `cargo nextest` to run tests.
* For re-usable code use a crate specific <crate_name>Error enum defined in error.rs with `thiserror`.
* Use `anyhow` for error handling in application code.
