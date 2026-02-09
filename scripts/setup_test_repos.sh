#!/usr/bin/env bash
set -euo pipefail

# Clone and pin test corpus repositories for keel integration testing.
# These repos provide known-good codebases at pinned commits for
# deterministic graph correctness testing.

CORPUS_DIR="${1:-test-corpus}"
mkdir -p "$CORPUS_DIR"

echo "Setting up test corpus in $CORPUS_DIR..."

# TODO: Clone specific repos at pinned commits
# Example:
# if [ ! -d "$CORPUS_DIR/express-example" ]; then
#     git clone https://github.com/example/express-example.git "$CORPUS_DIR/express-example"
#     (cd "$CORPUS_DIR/express-example" && git checkout abc1234)
# fi
#
# if [ ! -d "$CORPUS_DIR/flask-example" ]; then
#     git clone https://github.com/example/flask-example.git "$CORPUS_DIR/flask-example"
#     (cd "$CORPUS_DIR/flask-example" && git checkout def5678)
# fi
#
# if [ ! -d "$CORPUS_DIR/go-example" ]; then
#     git clone https://github.com/example/go-example.git "$CORPUS_DIR/go-example"
#     (cd "$CORPUS_DIR/go-example" && git checkout 789abcd)
# fi
#
# if [ ! -d "$CORPUS_DIR/rust-example" ]; then
#     git clone https://github.com/example/rust-example.git "$CORPUS_DIR/rust-example"
#     (cd "$CORPUS_DIR/rust-example" && git checkout 456efgh)
# fi

echo "Test corpus setup complete."
echo "Corpus directory: $CORPUS_DIR"
