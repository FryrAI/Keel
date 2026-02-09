#!/usr/bin/env bash
# keel installer — downloads the latest release binary for your platform.
#
# Usage:
#   curl -fsSL https://keel.engineer/install.sh | bash
#   # or with a specific version:
#   curl -fsSL https://keel.engineer/install.sh | bash -s -- v0.1.0

set -euo pipefail

REPO="keel-engineer/keel"
INSTALL_DIR="${KEEL_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${1:-latest}"

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

# Resolve version
if [ "$VERSION" = "latest" ]; then
    DOWNLOAD_URL="https://github.com/$REPO/releases/latest/download/$ARTIFACT"
else
    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/$ARTIFACT"
fi

echo "Installing keel ($PLATFORM/$ARCH_SUFFIX)..."
echo "  From: $DOWNLOAD_URL"
echo "  To:   $INSTALL_DIR/keel"

# Create install directory
mkdir -p "$INSTALL_DIR"

# Download
if command -v curl &>/dev/null; then
    curl -fsSL "$DOWNLOAD_URL" -o "$INSTALL_DIR/keel"
elif command -v wget &>/dev/null; then
    wget -q "$DOWNLOAD_URL" -O "$INSTALL_DIR/keel"
else
    echo "Error: curl or wget required" >&2
    exit 1
fi

chmod +x "$INSTALL_DIR/keel"

# Verify
if "$INSTALL_DIR/keel" --version &>/dev/null; then
    echo "✓ keel installed successfully: $("$INSTALL_DIR/keel" --version)"
else
    echo "✓ keel binary installed at $INSTALL_DIR/keel"
fi

# Check PATH
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
    echo ""
    echo "Note: $INSTALL_DIR is not in your PATH."
    echo "Add it with:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
fi
