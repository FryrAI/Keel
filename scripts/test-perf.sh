#!/usr/bin/env bash
set -euo pipefail
echo "Running performance tests in release mode..."
cargo test --features perf-tests --release 2>&1 | tail -30
echo "Done."
