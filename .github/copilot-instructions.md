# Copilot Custom Instructions for HermitGrab

## Rust Dependency Management

When suggesting or implementing the addition of Rust crates (dependencies), **always use the `cargo add` command** instead of directly editing the `Cargo.toml` file. This ensures that the latest compatible versions are used and that the dependency graph is updated correctly.

### Example

- To add the `serde` crate, use:
  ```sh
  cargo add serde
  ```
- To add a specific feature or version:
  ```sh
  cargo add clap --features derive
  cargo add anyhow@1.0
  ```

**Do not manually edit `Cargo.toml` to add dependencies.**

Prefer using crates that are well-maintained and widely used in the Rust ecosystem. 
Also prefer crates that don't rely on external tools or libraries.
