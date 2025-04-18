# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands
- Build (dev): `cargo build`
- Build (prod): `cargo build --release`
- Run node: `cargo run --bin littled`
- Run CLI: `cargo run --bin little-cli -- {command}`
- Run tests: `cargo test`
- Run specific test: `cargo test test_name`
- Lint: `cargo clippy`
- Format: `cargo fmt`

## Code Style Guidelines
- Use Rust 2021 edition standards
- Functions/variables: snake_case
- Structs/enums: PascalCase
- Constants: SCREAMING_SNAKE_CASE
- Use Result<T, E> for error handling
- Group imports by standard lib, external crates, then internal modules
- Document public APIs with detailed comments
- Use Tokio for async operations
- Separate logic into modules (commands.rs, config.rs, etc.)
- Use serde for serialization/deserialization
- Prefer strong typing over stringly typed interfaces