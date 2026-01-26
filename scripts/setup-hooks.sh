#!/bin/bash
# Setup git hooks for Port-Patrol

set -e

echo "Setting up pre-commit hooks..."

# Check if pre-commit is installed
if ! command -v pre-commit &> /dev/null; then
    echo "pre-commit not found. Installing..."
    if command -v pip &> /dev/null; then
        pip install pre-commit
    elif command -v pip3 &> /dev/null; then
        pip3 install pre-commit
    elif command -v brew &> /dev/null; then
        brew install pre-commit
    else
        echo "❌ Could not install pre-commit. Please install manually:"
        echo "   pip install pre-commit"
        echo "   or: brew install pre-commit"
        exit 1
    fi
fi

# Install the hooks
pre-commit install
pre-commit install --hook-type pre-push

echo ""
echo "✅ Pre-commit hooks installed!"
echo ""
echo "Hooks will run:"
echo "  On commit: cargo fmt, cargo clippy, file checks"
echo "  On push:   cargo test"
echo ""
echo "Commands:"
echo "  Run all hooks:  pre-commit run --all-files"
echo "  Skip hooks:     git commit --no-verify"
echo "  Update hooks:   pre-commit autoupdate"
