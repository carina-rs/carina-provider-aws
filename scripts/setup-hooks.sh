#!/bin/bash
# Setup git hooks for Carina

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Use git's common dir so the hook is shared across all worktrees
HOOKS_DIR="$(git rev-parse --git-common-dir)/hooks"
mkdir -p "$HOOKS_DIR"

echo "Installing git hooks..."

cp "$SCRIPT_DIR/pre-commit" "$HOOKS_DIR/pre-commit"
chmod +x "$HOOKS_DIR/pre-commit"

echo "Git hooks installed successfully!"
echo "Pre-commit hook will run: fmt, clippy"
