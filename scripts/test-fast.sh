#!/usr/bin/env bash
set -euo pipefail

echo "=== keel fast test suite ==="
echo "Running workspace unit tests..."
cargo test --workspace -- --include-ignored 2>&1 | head -50
echo ""
echo "=== Quick integration tests ==="
cargo test --test integration -- --include-ignored 2>&1 | head -20
echo ""
echo "=== Done ==="
