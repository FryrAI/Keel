#!/usr/bin/env bash
set -euo pipefail

echo "=== keel full test suite ==="
echo ""

echo "Phase 1: Unit tests"
cargo test --workspace
echo ""

echo "Phase 2: Integration tests"
cargo test --test integration -- --include-ignored
echo ""

echo "Phase 3: Oracle 1 - Graph correctness"
cargo test --test graph_correctness -- --include-ignored
echo ""

echo "Phase 4: Performance benchmarks"
cargo test --test benchmarks -- --include-ignored
echo ""

echo "=== All 4 oracles complete ==="
