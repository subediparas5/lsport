# Contributing to Lsport

Thank you for your interest in contributing to Lsport! This document provides guidelines and information for contributors.

## Code of Conduct

By participating in this project, you agree to maintain a respectful and inclusive environment for everyone.

## How to Contribute

### Reporting Bugs

1. Check if the bug has already been reported in [Issues](https://github.com/lsport/lsport/issues)
2. If not, create a new issue with:
   - Clear, descriptive title
   - Steps to reproduce
   - Expected vs actual behavior
   - System information (OS, Rust version)
   - Terminal emulator used

### Suggesting Features

1. Check existing issues and discussions for similar ideas
2. Create a new issue with the `enhancement` label
3. Describe the feature and its use case
4. Consider implementation complexity

### Pull Requests

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/your-feature`
3. Make your changes
4. Run tests: `cargo test`
5. Run lints: `cargo clippy`
6. Format code: `cargo fmt`
7. Commit with clear messages
8. Push and create a Pull Request

## Development Setup

```bash
# Clone your fork
git clone https://github.com/YOUR_USERNAME/lsport.git
cd lsport

# Install pre-commit hooks (recommended)
./scripts/setup-hooks.sh

# Build
cargo build

# Run tests
cargo test

# Run with debug output
RUST_LOG=debug cargo run

# Run clippy
cargo clippy -- -D warnings

# Format code
cargo fmt
```

### Pre-commit Hooks

We use [pre-commit](https://pre-commit.com/) for automated quality checks:

**On every commit:**
- `cargo fmt` - Formatting
- `cargo clippy` - Linting
- File checks (large files, merge conflicts, TOML/YAML syntax)

**On push:**
- `cargo test` - Full test suite

```bash
# Install hooks
./scripts/setup-hooks.sh

# Or manually
pip install pre-commit
pre-commit install
pre-commit install --hook-type pre-push

# Run all hooks manually
pre-commit run --all-files

# Skip temporarily
git commit --no-verify

# Update hooks
pre-commit autoupdate
```

## Code Style

- Follow Rust conventions and idioms
- Use `cargo fmt` for formatting
- Use `cargo clippy` for linting
- Write documentation for public APIs
- Add tests for new functionality
- Keep functions focused and small

## Architecture

Lsport follows the Model-View-Update (MVU) pattern:

- `main.rs` - Entry point, CLI parsing, event loop
- `app.rs` - Application state (Model)
- `ui.rs` - Rendering logic (View)
- `scanner.rs` - Local port scanning
- `remote.rs` - SSH remote scanning

## Testing

- Unit tests go in the same file as the code
- Integration tests go in `tests/`
- Aim for good coverage of edge cases
- Test both success and error paths

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture
```

## Commit Messages

Use clear, descriptive commit messages:

```
feat: add UDP port scanning support
fix: handle permission denied errors gracefully
docs: update README with new CLI options
test: add tests for zombie detection
refactor: extract SSH connection logic to separate module
```

## Questions?

Feel free to open an issue for any questions about contributing!
