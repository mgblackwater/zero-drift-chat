# Contributing to zero-drift-chat

Thanks for your interest in contributing! This document covers how to build, test, and submit changes.

## Prerequisites

- **Rust nightly** — the project uses `rust-toolchain.toml` to pin the toolchain, so `rustup` will install the correct version automatically.
- `cargo` (comes with Rust)

Install Rust via [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Building

```bash
# Clone the repo
git clone git@github.com:mgblackwater/zero-drift-chat.git
cd zero-drift-chat

# Debug build
cargo build

# Release build
cargo build --release
```

The compiled binary lands at `target/debug/zero-drift-chat` (or `target/release/` for release).

## Running Tests

```bash
# Run all tests
cargo test

# Run tests with output visible
cargo test -- --nocapture

# Run a specific test
cargo test <test_name>
```

## Code Style

This project follows standard Rust formatting conventions:

```bash
# Format code (required before committing)
cargo fmt

# Lint with Clippy
cargo clippy -- -D warnings
```

CI will reject PRs that fail `cargo fmt --check` or have Clippy warnings.

## Submitting a Pull Request

1. **Fork** the repo and create your branch from `master`:
   ```bash
   git checkout -b feat/your-feature-name
   ```

2. **Make your changes.** Keep commits focused and atomic.

3. **Run the full check suite locally:**
   ```bash
   cargo fmt --check
   cargo clippy -- -D warnings
   cargo test
   ```

4. **Write a clear commit message.** We follow [Conventional Commits](https://www.conventionalcommits.org/):
   - `feat: add X` for new features
   - `fix: resolve Y` for bug fixes
   - `docs: update Z` for documentation
   - `chore: ...` for tooling / maintenance

5. **Push your branch** and open a PR against `master`.

6. **Fill in the PR description** — explain *what* changed and *why*.

7. A maintainer will review your PR. Address feedback and push additional commits to the same branch.

## Reporting Issues

Open an [issue](https://github.com/mgblackwater/zero-drift-chat/issues) with:
- A clear title and description
- Steps to reproduce (if it's a bug)
- Your OS, Rust version (`rustc --version`), and app version
