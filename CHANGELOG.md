# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2024-01-26

### Added
- Initial release of Port-Patrol
- Live port monitoring with real-time updates
- TCP and UDP port scanning support
- Remote server monitoring via SSH (`--host` flag)
- Interactive TUI with k9s-inspired design
- Process termination with Enter key
- Regex-based filtering (`/` key)
- K9s-style sorting (Shift+P/O/I/N/C/M or 1-6 keys)
- Zombie process detection (high CPU + orphaned)
- Help popup (`?` key)
- Vim-style navigation (j/k keys)
- Custom scan interval (`-s` flag)
- SSH key authentication support (`-i` flag)
- Color-coded protocols (TCP=blue, UDP=green)
- CPU usage color indicators
- Sort indicators in column headers

### Security
- Graceful permission error handling
- No credential storage - uses SSH agent or key files

[Unreleased]: https://github.com/port-patrol/port-patrol/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/port-patrol/port-patrol/releases/tag/v0.1.0
