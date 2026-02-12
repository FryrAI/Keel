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
    "axum-v0.8.0"

# ripgrep — workspace, complex module tree (~30k LOC)
clone_and_pin \
    "https://github.com/BurntSushi/ripgrep.git" \
    "ripgrep" \
    "14.1.1"

# serde — derive macros, trait impls (~15k LOC)
clone_and_pin \
    "https://github.com/serde-rs/serde.git" \
    "serde" \
    "v1.0.217"

# --- Extended TypeScript ---
echo ""
echo "=== Extended TypeScript ==="

# trpc — monorepo, cross-package imports (~40k LOC)
clone_and_pin \
    "https://github.com/trpc/trpc.git" \
    "trpc" \
    "v11.9.0"

# zod — generics, method chaining (~10k LOC)
clone_and_pin \
    "https://github.com/colinhacks/zod.git" \
    "zod" \
    "v3.24.1"

# --- Extended Python ---
echo ""
echo "=== Extended Python ==="

# pydantic — metaclass, validators (~40k LOC)
clone_and_pin \
    "https://github.com/pydantic/pydantic.git" \
    "pydantic" \
    "v2.10.6"

# flask — blueprints, extension patterns (~15k LOC)
clone_and_pin \
    "https://github.com/pallets/flask.git" \
    "flask" \
    "3.1.0"

# --- Extended Go ---
echo ""
echo "=== Extended Go ==="

# gin — HTTP framework, embedded types (~20k LOC)
clone_and_pin \
    "https://github.com/gin-gonic/gin.git" \
    "gin" \
    "v1.10.0"

# fzf — complex CLI, goroutine patterns (~25k LOC)
clone_and_pin \
    "https://github.com/junegunn/fzf.git" \
    "fzf" \
    "v0.57.0"

echo ""
echo "Test corpus setup complete."
echo "Corpus directory: $CORPUS_DIR"
echo ""
echo "Repos cloned:"
ls -1 "$CORPUS_DIR"
