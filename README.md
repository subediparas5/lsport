# lsport APT Repository

APT repository for [lsport](https://github.com/subediparas5/lsport) — A TUI for managing local and remote ports via SSH.

## Quick Install

```bash
curl -fsSL https://subediparas5.github.io/lsport/install.sh | bash
```

## Manual Install

```bash
# Add GPG key
curl -fsSL https://subediparas5.github.io/lsport/KEY.gpg | sudo gpg --dearmor -o /usr/share/keyrings/lsport.gpg

# Add repository
echo "deb [signed-by=/usr/share/keyrings/lsport.gpg arch=$(dpkg --print-architecture)] https://subediparas5.github.io/lsport stable main" | sudo tee /etc/apt/sources.list.d/lsport.list

# Install
sudo apt update
sudo apt install lsport
```

## Update

```bash
sudo apt update && sudo apt upgrade lsport
```

## Uninstall

```bash
sudo apt remove lsport
sudo rm /etc/apt/sources.list.d/lsport.list
sudo rm /usr/share/keyrings/lsport.gpg
```

## Other Installation Methods

- **Homebrew:** `brew install subediparas5/tap/lsport`
- **Cargo:** `cargo install lsport`
- **Binary:** Download from [releases](https://github.com/subediparas5/lsport/releases)
