---
applyTo: '**/*.rs'
---
# Rust Code Style Guide
* Use `cargo fmt` to format your code.
* Use `cargo clippy` to lint your code.
* Use `cargo nextest run` to run tests.
* For re-usable code use a crate specific <crate_name>Error enum defined in error.rs with `thiserror`.
* Use `anyhow` for error handling in application code.
* Use `serde_yaml_ng`instead of `serde_yaml` for YAML parsing.
* For format strings, use the "{varA}" syntax, instead of "{}", varA when possible.
* Use `clap` derive for command line argument parsing.
* Use `tokio` for async programming.
* Use `reqwest` for HTTP requests.
* Use `serde` for serialization and deserialization.
* Use `tracing` for structured logging.
* When wrapping an error, use #[error(transparent)] to preserve the original error message.