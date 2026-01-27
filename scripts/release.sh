#!/bin/bash
# Release script for Lsport
# Usage: ./scripts/release.sh [major|minor|patch] or ./scripts/release.sh 1.2.3

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Get current version from Cargo.toml
CURRENT_VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
echo -e "${YELLOW}Current version: ${NC}$CURRENT_VERSION"

# Parse current version
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"

# Determine new version
if [ -z "$1" ]; then
    echo -e "${RED}Usage: $0 [major|minor|patch|x.y.z]${NC}"
    echo "  major - Bump major version (1.0.0 -> 2.0.0)"
    echo "  minor - Bump minor version (1.0.0 -> 1.1.0)"
    echo "  patch - Bump patch version (1.0.0 -> 1.0.1)"
    echo "  x.y.z - Set specific version"
    exit 1
fi

case "$1" in
    major)
        NEW_VERSION="$((MAJOR + 1)).0.0"
        ;;
    minor)
        NEW_VERSION="${MAJOR}.$((MINOR + 1)).0"
        ;;
    patch)
        NEW_VERSION="${MAJOR}.${MINOR}.$((PATCH + 1))"
        ;;
    *)
        # Validate semver format
        if [[ ! "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$ ]]; then
            echo -e "${RED}Invalid version format. Use x.y.z or x.y.z-prerelease${NC}"
            exit 1
        fi
        NEW_VERSION="$1"
        ;;
esac

echo -e "${GREEN}New version: ${NC}$NEW_VERSION"

# Check for uncommitted changes
if [ -n "$(git status --porcelain)" ]; then
    echo -e "${RED}Error: You have uncommitted changes. Please commit or stash them first.${NC}"
    exit 1
fi

# Check we're on main branch
BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [ "$BRANCH" != "main" ]; then
    echo -e "${YELLOW}Warning: You're on branch '$BRANCH', not 'main'.${NC}"
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Check if tag already exists
if git rev-parse "v$NEW_VERSION" >/dev/null 2>&1; then
    echo -e "${RED}Error: Tag v$NEW_VERSION already exists!${NC}"
    exit 1
fi

# Confirm
echo ""
echo "This will:"
echo "  1. Update version in Cargo.toml to $NEW_VERSION"
echo "  2. Update Cargo.lock"
echo "  3. Commit changes"
echo "  4. Create tag v$NEW_VERSION"
echo "  5. Push to origin"
echo ""
read -p "Proceed? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 1
fi

# Update Cargo.toml
echo -e "${YELLOW}Updating Cargo.toml...${NC}"
sed -i.bak "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml
rm -f Cargo.toml.bak

# Update Cargo.lock
echo -e "${YELLOW}Updating Cargo.lock...${NC}"
cargo check --quiet

# Commit
echo -e "${YELLOW}Committing...${NC}"
git add Cargo.toml Cargo.lock
git commit -m "chore: release v$NEW_VERSION [skip ci]"

# Tag
echo -e "${YELLOW}Creating tag v$NEW_VERSION...${NC}"
git tag -a "v$NEW_VERSION" -m "Release v$NEW_VERSION"

# Push
echo -e "${YELLOW}Pushing to origin...${NC}"
git push origin "$BRANCH"
git push origin "v$NEW_VERSION"

echo ""
echo -e "${GREEN}✅ Released v$NEW_VERSION!${NC}"
echo ""
echo "GitHub Actions will now:"
echo "  - Create a GitHub Release"
echo "  - Build binaries for all platforms"
echo "  - Publish to crates.io (if configured)"
echo ""
echo "View release: https://github.com/$(git remote get-url origin | sed 's/.*github.com[:/]\(.*\)\.git/\1/')/releases/tag/v$NEW_VERSION"
