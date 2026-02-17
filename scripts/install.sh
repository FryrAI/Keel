#!/usr/bin/env bash
# keel installer — downloads the latest release binary for your platform.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/FryrAI/Keel/main/scripts/install.sh | bash
#   # or via public gist (available while repo is private):
#   curl -fsSL https://gist.githubusercontent.com/FryrAI/fe93164768d13aaa8dcdf68ad1ce6439/raw/install.sh | bash
#   # with a specific version:
#   curl -fsSL https://raw.githubusercontent.com/FryrAI/Keel/main/scripts/install.sh | bash -s -- v0.1.0
#
# Environment variables:
#   KEEL_INSTALL_DIR — override install directory (default: ~/.local/bin)
#   KEEL_SKIP_CHECKSUM — set to 1 to skip checksum verification

set -euo pipefail

REPO="FryrAI/Keel"
INSTALL_DIR="${KEEL_INSTALL_DIR:-$HOME/.local/bin}"
SKIP_CHECKSUM="${KEEL_SKIP_CHECKSUM:-0}"

# Parse arguments
VERSION="latest"
for arg in "$@"; do
    case "$arg" in
        --version) echo "keel installer v0.1.0"; exit 0 ;;
        v*) VERSION="$arg" ;;
        *) VERSION="$arg" ;;
    esac
done

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  PLATFORM="linux" ;;
    Darwin) PLATFORM="darwin" ;;
    *)
        echo "Error: unsupported OS: $OS" >&2
        echo "Windows users: download from https://github.com/$REPO/releases" >&2
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64|amd64) ARCH_SUFFIX="amd64" ;;
    aarch64|arm64) ARCH_SUFFIX="arm64" ;;
    *)
        echo "Error: unsupported architecture: $ARCH" >&2
        exit 1
        ;;
esac

ARTIFACT="keel-${PLATFORM}-${ARCH_SUFFIX}"

# Resolve version and URLs
if [ "$VERSION" = "latest" ]; then
    DOWNLOAD_URL="https://github.com/$REPO/releases/latest/download/$ARTIFACT"
    CHECKSUM_URL="https://github.com/$REPO/releases/latest/download/checksums-sha256.txt"
else
    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/$ARTIFACT"
    CHECKSUM_URL="https://github.com/$REPO/releases/download/$VERSION/checksums-sha256.txt"
fi

echo "Installing keel ($PLATFORM/$ARCH_SUFFIX)..."
echo "  From: $DOWNLOAD_URL"
echo "  To:   $INSTALL_DIR/keel"

# Create install directory
mkdir -p "$INSTALL_DIR"

# Download helper
download() {
    local url="$1" dest="$2"
    if command -v curl &>/dev/null; then
        curl -fsSL "$url" -o "$dest"
    elif command -v wget &>/dev/null; then
        wget -q "$url" -O "$dest"
    else
        echo "Error: curl or wget required" >&2
        exit 1
    fi
}

# Download binary
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

download "$DOWNLOAD_URL" "$TMPDIR/keel"

# Verify checksum
if [ "$SKIP_CHECKSUM" != "1" ]; then
    echo "Verifying checksum..."
    download "$CHECKSUM_URL" "$TMPDIR/checksums.txt"

    EXPECTED=$(grep "$ARTIFACT" "$TMPDIR/checksums.txt" | awk '{print $1}')
    if [ -z "$EXPECTED" ]; then
        echo "Warning: no checksum found for $ARTIFACT, skipping verification" >&2
    else
        if command -v sha256sum &>/dev/null; then
            ACTUAL=$(sha256sum "$TMPDIR/keel" | awk '{print $1}')
        elif command -v shasum &>/dev/null; then
            ACTUAL=$(shasum -a 256 "$TMPDIR/keel" | awk '{print $1}')
        else
            echo "Warning: no sha256sum or shasum found, skipping verification" >&2
            ACTUAL="$EXPECTED"
        fi

        if [ "$EXPECTED" != "$ACTUAL" ]; then
            echo "Error: checksum mismatch!" >&2
            echo "  Expected: $EXPECTED" >&2
            echo "  Actual:   $ACTUAL" >&2
            exit 1
        fi
        echo "  Checksum verified."
    fi
fi

# Install binary
cp "$TMPDIR/keel" "$INSTALL_DIR/keel"
chmod +x "$INSTALL_DIR/keel"

# Verify
if "$INSTALL_DIR/keel" --version &>/dev/null; then
    echo "keel installed successfully: $("$INSTALL_DIR/keel" --version)"
else
    echo "keel binary installed at $INSTALL_DIR/keel"
fi

# Check PATH
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
    echo ""
    echo "Note: $INSTALL_DIR is not in your PATH."
    SHELL_NAME="$(basename "${SHELL:-/bin/bash}")"
    case "$SHELL_NAME" in
        zsh)  RC_FILE="~/.zshrc" ;;
        fish) RC_FILE="~/.config/fish/config.fish" ;;
        *)    RC_FILE="~/.bashrc" ;;
    esac
    echo "Add it with:"
    echo "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> $RC_FILE"
fi

echo ""
echo "Get started:"
echo "  cd your-project && keel init && keel map"
echo ""
echo "Shell completions:"
SHELL_NAME="$(basename "${SHELL:-/bin/bash}")"
case "$SHELL_NAME" in
    zsh)  echo "  keel completion zsh > ~/.zfunc/_keel" ;;
    fish) echo "  keel completion fish > ~/.config/fish/completions/keel.fish" ;;
    *)    echo "  keel completion bash > /etc/bash_completion.d/keel" ;;
esac
echo ""
echo "Star us: gh star FryrAI/Keel"
