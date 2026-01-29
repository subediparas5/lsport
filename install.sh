#!/bin/bash
set -e
echo "Installing lsport..."
curl -fsSL https://subediparas5.github.io/lsport/KEY.gpg | sudo gpg --dearmor -o /usr/share/keyrings/lsport.gpg
echo "deb [signed-by=/usr/share/keyrings/lsport.gpg arch=$(dpkg --print-architecture)] https://subediparas5.github.io/lsport stable main" | sudo tee /etc/apt/sources.list.d/lsport.list
sudo apt update && sudo apt install -y lsport
echo "✅ lsport installed!"
