#!/bin/bash
set -e

echo "Installing lsport APT repository..."

# Download and install GPG key
curl -fsSL https://subediparas5.github.io/lsport/KEY.gpg | sudo gpg --dearmor -o /usr/share/keyrings/lsport.gpg

# Add repository
echo "deb [signed-by=/usr/share/keyrings/lsport.gpg arch=$(dpkg --print-architecture)] https://subediparas5.github.io/lsport stable main" | sudo tee /etc/apt/sources.list.d/lsport.list

# Update and install
sudo apt update
sudo apt install -y lsport

echo "✅ lsport installed successfully!"
