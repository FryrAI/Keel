#!/usr/bin/env bash
set -euo pipefail

# Capture full test output to files, then head the files for display.
# Piping `cargo test | head` directly would send SIGPIPE to cargo once head
# closes the pipe (even on a PASSING run with long output), which pipefail +
# set -e turn into a spurious script failure. Capturing first lets cargo run to
# completion and report its true exit code.
log_dir="$(mktemp -d)"
trap 'rm -rf "$log_dir"' EXIT

run_tests() {
	local label="$1" log="$2" lines="$3"
	shift 3
	local status=0
	"$@" >"$log" 2>&1 || status=$?
	head -"$lines" "$log"
	if [ "$status" -ne 0 ]; then
		echo "$label FAILED (exit $status)"
		exit "$status"
	fi
}

echo "=== keel fast test suite ==="
echo "Running workspace unit tests..."
run_tests "workspace tests" "$log_dir/workspace.log" 50 \
	cargo test --workspace -- --include-ignored
echo ""
echo "=== Quick integration tests ==="
run_tests "integration tests" "$log_dir/integration.log" 20 \
	cargo test --test integration -- --include-ignored
echo ""
echo "=== Done ==="
