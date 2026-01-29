#!/bin/bash
# Local CI test script - mirrors GitHub Actions CI workflow
# Runs CI tasks: debian build
# Note: Build, MSRV, docs, and audit checks are handled by pre-commit hooks
# Usage: ./scripts/ci-local.sh [--skip-debian] [--fast]

set +e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Parse arguments
SKIP_DEBIAN=false
FAST=false

for arg in "$@"; do
    case $arg in
        --skip-debian)
            SKIP_DEBIAN=true
            shift
            ;;
        --fast)
            FAST=true
            SKIP_DEBIAN=true
            shift
            ;;
        *)
            echo -e "${RED}Unknown option: $arg${NC}"
            echo "Usage: $0 [--skip-debian] [--fast]"
            exit 1
            ;;
    esac
done

# Set environment variables (matching CI)
export CARGO_TERM_COLOR=always
export RUST_BACKTRACE=1
export CARGO_INCREMENTAL=0
export CARGO_NET_RETRY=10
export RUSTUP_MAX_RETRIES=10

# Track failures
FAILED=0
TOTAL_CHECKS=0

# Helper function to run a check
run_check() {
    local name=$1
    local command=$2
    local optional=${3:-false}

    TOTAL_CHECKS=$((TOTAL_CHECKS + 1))
    echo -e "\n${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}Running: ${NC}${YELLOW}$name${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

    if eval "$command"; then
        echo -e "${GREEN}✅ $name passed${NC}"
        return 0
    else
        echo -e "${RED}❌ $name failed${NC}"
        if [ "$optional" = "true" ]; then
            echo -e "${YELLOW}⚠️  $name is optional, continuing...${NC}"
            return 0
        else
            FAILED=$((FAILED + 1))
            return 1
        fi
    fi
}

# Get Rust version
RUST_VERSION=$(rustc --version 2>/dev/null || echo "not installed")
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Local CI Test Runner${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "Rust version: ${YELLOW}$RUST_VERSION${NC}"
echo -e "Working directory: ${YELLOW}$(pwd)${NC}"
echo ""

# 1. Debian build check (optional)
if [ "$SKIP_DEBIAN" = false ]; then
    if [ -d "debian" ]; then
        # Check if we can build with debian/rules (requires make, cargo, and dh)
        if command -v make &> /dev/null && command -v cargo &> /dev/null && command -v dh &> /dev/null; then
            if [ -f "debian/rules" ]; then
                # Run debian build check (clean + build)
                run_check "Debian Build Check" "make -f debian/rules clean && make -f debian/rules build" true
            else
                echo -e "\n${YELLOW}⚠️  debian/rules not found. Skipping Debian build...${NC}"
            fi
        else
            echo -e "\n${YELLOW}⚠️  Required tools (make/cargo/dh) not found. Skipping Debian build...${NC}"
            echo -e "   Note: 'dh' (debhelper) is required for Debian builds and is typically only available on Debian/Ubuntu systems"
        fi

        # Run lintian if available (on any .deb files)
        if command -v lintian &> /dev/null; then
            DEB_FILES=$(find . -maxdepth 1 -name "*.deb" 2>/dev/null | head -1)
            if [ -n "$DEB_FILES" ]; then
                run_check "Lintian (Debian Package Linter)" "lintian --info $DEB_FILES" true
            fi
        fi
    else
        echo -e "\n${YELLOW}⏭️  No debian/ directory found. Skipping Debian build...${NC}"
    fi
else
    echo -e "\n${YELLOW}⏭️  Skipping Debian build check (--skip-debian)${NC}"
fi

# Summary
echo -e "\n${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}✅ All required checks passed! ($TOTAL_CHECKS/$TOTAL_CHECKS)${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    exit 0
else
    echo -e "${RED}❌ $FAILED of $TOTAL_CHECKS required checks failed${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    exit 1
fi
