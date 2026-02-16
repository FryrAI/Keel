#!/usr/bin/env bash
# keel GitHub Action entrypoint — downloads and installs the keel binary.
# Usage: entrypoint.sh install
set -euo pipefail

REPO="FryrAI/Keel"
CACHE_DIR="$HOME/.keel-bin"

# ---------------------------------------------------------------------------
# Detect platform
# ---------------------------------------------------------------------------
detect_platform() {
  local os arch
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"

  case "$os" in
    linux)  os="linux" ;;
    darwin) os="darwin" ;;
    *)      echo "::error::Unsupported OS: $os" && exit 2 ;;
  esac

  case "$arch" in
    x86_64)  arch="amd64" ;;
    aarch64) arch="arm64" ;;
    arm64)   arch="arm64" ;;
    *)       echo "::error::Unsupported architecture: $arch" && exit 2 ;;
  esac

  echo "${os}-${arch}"
}

# ---------------------------------------------------------------------------
# Resolve version (latest → actual tag)
# ---------------------------------------------------------------------------
resolve_version() {
  local version="${KEEL_VERSION:-latest}"
  if [ "$version" = "latest" ]; then
    version=$(curl -fsSL \
      -H "Authorization: token ${GITHUB_TOKEN}" \
      -H "Accept: application/vnd.github+json" \
      "https://api.github.com/repos/${REPO}/releases/latest" \
      | grep '"tag_name"' | sed -E 's/.*"tag_name":\s*"v?([^"]+)".*/\1/')
    if [ -z "$version" ]; then
      echo "::error::Failed to resolve latest keel version"
      exit 2
    fi
  fi
  # Strip leading 'v' if present
  version="${version#v}"
  echo "$version"
}

# ---------------------------------------------------------------------------
# Install
# ---------------------------------------------------------------------------
install() {
  local platform version binary_name url dest

  platform="$(detect_platform)"
  version="$(resolve_version)"
  binary_name="keel-${platform}"
  dest="${CACHE_DIR}/keel-${version}"

  echo "::group::Install keel v${version} (${platform})"

  # Cache hit — skip download
  if [ -x "$dest" ]; then
    echo "Cache hit: ${dest}"
  else
    mkdir -p "$CACHE_DIR"
    url="https://github.com/${REPO}/releases/download/v${version}/${binary_name}"
    echo "Downloading ${url}"
    curl -fsSL \
      -H "Authorization: token ${GITHUB_TOKEN}" \
      -H "Accept: application/octet-stream" \
      -o "$dest" \
      "$url"
    chmod +x "$dest"
  fi

  # Symlink into a PATH-friendly location
  sudo ln -sf "$dest" /usr/local/bin/keel

  echo "Installed keel v${version}"
  keel --version
  echo "::endgroup::"
}

# ---------------------------------------------------------------------------
# Dispatch
# ---------------------------------------------------------------------------
case "${1:-install}" in
  install) install ;;
  *)       echo "::error::Unknown command: $1" && exit 2 ;;
esac
