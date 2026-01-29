#!/bin/bash
# Script to test Debian build in Docker
# Usage: ./scripts/test-debian-docker.sh

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Testing Debian Build in Docker${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

# Get the project root directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Build Docker image
echo -e "\n${YELLOW}Building Docker image...${NC}"
docker build -f "$PROJECT_ROOT/Dockerfile.debian-test" -t lsport-debian-test "$PROJECT_ROOT"

# Run the container - test Debian build only
echo -e "\n${YELLOW}Testing Debian build in Docker container...${NC}"
if docker run --rm lsport-debian-test; then
    echo -e "\n${GREEN}✅ Debian build test passed!${NC}"
    exit 0
else
    echo -e "\n${RED}❌ Debian build test failed!${NC}"
    exit 1
fi
