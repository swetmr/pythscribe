#!/usr/bin/env bash
# One-time (per clone) hook install: point git at the committed .githooks/ dir.
# Re-running is harmless. See CONTRIBUTING.md.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
git config core.hooksPath .githooks
chmod +x .githooks/* 2>/dev/null || true
echo "git hooks installed (core.hooksPath = .githooks):"
ls -1 .githooks
