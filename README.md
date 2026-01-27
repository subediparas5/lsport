# ⚓ Lsport

A TUI for managing local and remote ports via SSH. Quickly identify which process is hogging a port and kill it instantly.

[![CI](https://github.com/subediparas5/lsport/actions/workflows/ci.yml/badge.svg)](https://github.com/subediparas5/lsport/actions/workflows/ci.yml)
[![Release](https://github.com/subediparas5/lsport/actions/workflows/release.yml/badge.svg)](https://github.com/subediparas5/lsport/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)

## Features

- 📡 **Live Port Monitor** - Real-time table displaying ports, protocols (TCP/UDP), PIDs, process names, CPU%, and memory usage
- 🔌 **TCP & UDP Support** - Scans both TCP and UDP listening ports using multiple detection methods
- 🌐 **Remote Monitoring** - Monitor ports on remote servers via SSH
- ⌨️ **Interactive Navigation** - Vim-style navigation with arrow keys or `j`/`k`
- 💀 **Process Termination** - Kill processes directly from the TUI with a single keystroke
- 🔍 **Regex Filtering** - Filter ports by name, PID, or port number with regex support
- 🧟 **Zombie Detection** - Automatically highlights suspicious processes (high CPU + orphaned) in red
- 🎨 **K9s-Inspired UI** - Beautiful dark theme with color-coded information

## Installation

### macOS (Homebrew)

```bash
brew install subediparas5/tap/lsport
```

### Linux (Debian/Ubuntu)

```bash
curl -fsSL https://subediparas5.github.io/lsport/install.sh | bash
```

Or manually:
```bash
# Add GPG key and repository
curl -fsSL https://subediparas5.github.io/lsport/KEY.gpg | sudo gpg --dearmor -o /usr/share/keyrings/lsport.gpg
echo "deb [signed-by=/usr/share/keyrings/lsport.gpg arch=$(dpkg --print-architecture)] https://subediparas5.github.io/lsport stable main" | sudo tee /etc/apt/sources.list.d/lsport.list
sudo apt update && sudo apt install lsport
```

### Cargo (Any platform)

```bash
cargo install lsport
```

### From Source

```bash
git clone https://github.com/subediparas5/lsport.git
cd lsport
cargo build --release
sudo cp target/release/lsport /usr/local/bin/
```

### Binary Download

Download pre-built binaries from [GitHub Releases](https://github.com/subediparas5/lsport/releases).

## Usage

```bash
# Monitor localhost (default)
lsport

# For killing system processes, you may need sudo
sudo lsport

# Monitor a remote server via SSH
lsport --host user@example.com

# Remote server with custom SSH port
lsport --host user@example.com:2222

# Use a specific SSH key
lsport --host user@example.com -i ~/.ssh/my_key

# Custom scan interval (5 seconds)
lsport -s 5
```

### CLI Options

| Option | Description |
|--------|-------------|
| `-H, --host <HOST>` | Remote host (format: `user@host:port` or `user@host` or `host`) |
| `-i, --identity <PATH>` | Path to SSH private key |
| `-s, --scan-interval <SECS>` | Scan interval in seconds (default: 2) |
| `-h, --help` | Print help |
| `-V, --version` | Print version |

### Keybindings

| Key | Action |
|-----|--------|
| `↑` / `k` | Move selection up |
| `↓` / `j` | Move selection down |
| `PgUp` | Move up 10 rows |
| `PgDn` | Move down 10 rows |
| `Home` | Go to first entry |
| `End` | Go to last entry |
| `Enter` | Kill selected process |
| `/` | Enter filter mode (supports regex!) |
| `?` | Toggle help popup |
| `Esc` | Clear filter / Close help |
| `q` | Quit |

### K9s-Style Sorting

| Key | Action |
|-----|--------|
| `Shift+P` / `1` | Sort by **P**ort |
| `Shift+O` / `2` | Sort by Pr**o**tocol |
| `Shift+I` / `3` | Sort by P**I**D |
| `Shift+N` / `4` | Sort by **N**ame |
| `Shift+C` / `5` | Sort by **C**PU % |
| `Shift+M` / `6` | Sort by **M**emory |

*Press the same key again to toggle ascending/descending order.*

### SSH Authentication

For remote monitoring, authentication is attempted in this order:
1. Specified key (`-i` flag)
2. SSH agent (if running)
3. Default keys: `~/.ssh/id_ed25519`, `~/.ssh/id_rsa`, `~/.ssh/id_ecdsa`

### Filtering

Press `/` to enter filter mode. Filters support:

- **Simple text**: `node` matches any entry containing "node"
- **Regex patterns**: `^80[0-9]{2}$` matches ports 8000-8099
- **Case-insensitive**: All filters are case-insensitive

The context bar shows "Regex:" when your filter is a valid regex pattern.

## UI Design

Lsport features a **k9s-inspired** terminal UI with:

- 🎨 **Dark theme** with blue/cyan accents
- 📊 **Clean table** with alternating row colors
- 🔍 **Sort indicators** in column headers `[P/1]▲`
- ⌨️ **Vim-style** command bar at bottom
- 📋 **Help popup** (`?`) with full keybinding reference
- 🏷️ **Color-coded** protocols (TCP=blue, UDP=green) and CPU usage

## Development

### Prerequisites

- Rust 1.85.0 or later
- OpenSSL development libraries (for SSH support)

### Setup

```bash
# Install pre-commit hooks
./scripts/setup-hooks.sh

# Or manually with pre-commit
pip install pre-commit
pre-commit install
pre-commit install --hook-type pre-push
```

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Run clippy
cargo clippy --all-targets --all-features -- -D warnings

# Format code
cargo fmt
```

### Project Structure

```
src/
├── main.rs      # Entry point, CLI parsing & event loop
├── app.rs       # Model - Application state management
├── scanner.rs   # Local port scanning & process correlation
├── remote.rs    # SSH remote scanning module
└── ui.rs        # View - Ratatui rendering logic
```

### Releasing

```bash
# Patch release (0.1.0 → 0.1.1)
./scripts/release.sh patch

# Minor release (0.1.0 → 0.2.0)
./scripts/release.sh minor

# Major release (0.1.0 → 1.0.0)
./scripts/release.sh major
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| [ratatui](https://crates.io/crates/ratatui) | TUI framework |
| [crossterm](https://crates.io/crates/crossterm) | Terminal backend |
| [sysinfo](https://crates.io/crates/sysinfo) | System/process information |
| [listeners](https://crates.io/crates/listeners) | Port to PID mapping |
| [ssh2](https://crates.io/crates/ssh2) | SSH remote connections |
| [clap](https://crates.io/crates/clap) | CLI argument parsing |
| [regex](https://crates.io/crates/regex) | Filter pattern matching |
| [anyhow](https://crates.io/crates/anyhow) | Error handling |

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## Security

For security concerns, please see [SECURITY.md](SECURITY.md).

## License

MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

- Inspired by [k9s](https://k9scli.io/) for the UI design
- Built with [Ratatui](https://ratatui.rs/)
