#!/usr/bin/env bash
set -euo pipefail

# Clone and pin test corpus repositories for keel integration testing.
# These repos provide known-good codebases at pinned commits for
# deterministic graph correctness testing.
#
# Usage: ./scripts/setup_test_repos.sh [corpus-dir]
# Default corpus dir: test-corpus/

CORPUS_DIR="${1:-test-corpus}"
mkdir -p "$CORPUS_DIR"

echo "Setting up test corpus in $CORPUS_DIR..."

clone_and_pin() {
    local repo="$1"
    local name="$2"
    local sha="$3"
    local dest="$CORPUS_DIR/$name"

    if [ -d "$dest" ]; then
        echo "  [skip] $name already exists"
        return
    fi

    echo "  [clone] $name from $repo @ $sha"
    git clone --quiet "$repo" "$dest"
    (cd "$dest" && git checkout --quiet "$sha")
    echo "  [done] $name"
}

# --- TypeScript ---
echo ""
echo "=== TypeScript ==="

# ky — fast HTTP client, well-typed, cross-file imports (~3k LOC)
clone_and_pin \
    "https://github.com/sindresorhus/ky.git" \
    "ky" \
    "v1.7.5"

# zustand — state management, barrel exports, re-exports (~5k LOC)
clone_and_pin \
    "https://github.com/pmndrs/zustand.git" \
    "zustand" \
    "v5.0.3"

# --- Python ---
echo ""
echo "=== Python ==="

# httpx — excellent type hints, clean module structure (~25k LOC)
clone_and_pin \
    "https://github.com/encode/httpx.git" \
    "httpx" \
    "0.28.1"

# fastapi — pydantic types, endpoint detection (~30k LOC)
clone_and_pin \
    "https://github.com/fastapi/fastapi.git" \
    "fastapi" \
    "0.115.8"

# --- Go ---
echo ""
echo "=== Go ==="

# cobra — clean Go module structure (~15k LOC)
clone_and_pin \
    "https://github.com/spf13/cobra.git" \
    "cobra" \
    "v1.8.1"

# fiber — HTTP framework, route definitions (~30k LOC)
clone_and_pin \
    "https://github.com/gofiber/fiber.git" \
    "fiber" \
    "v2.52.6"

# --- Rust ---
echo ""
echo "=== Rust ==="

# axum — web framework, well-structured workspace (~20k LOC)
clone_and_pin \
    "https://github.com/tokio-rs/axum.git" \
    "axum" \
    "axum-v0.8.1"

echo ""
echo "Test corpus setup complete."
echo "Corpus directory: $CORPUS_DIR"
echo ""
echo "Repos cloned:"
ls -1 "$CORPUS_DIR"
