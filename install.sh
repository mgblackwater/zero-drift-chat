#!/bin/bash
set -euo pipefail

REPO="mgblackwater/zero-drift-chat"
INSTALL_DIR="$HOME/.local/bin"

echo "Installing zero-drift-chat..."

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)
    ASSET="zero-drift-chat-macos-aarch64"
    ;;
  Linux)
    echo "Error: No Linux binary available yet. Build from source:"
    echo "  cargo install --git https://github.com/$REPO"
    exit 1
    ;;
  MINGW*|MSYS*|CYGWIN*)
    echo "On Windows, use PowerShell instead:"
    echo "  irm https://raw.githubusercontent.com/$REPO/master/install.ps1 | iex"
    exit 1
    ;;
  *)
    echo "Error: Unsupported OS: $OS"
    exit 1
    ;;
esac

# Get latest release download URL
DOWNLOAD_URL="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
  | grep "browser_download_url.*$ASSET" \
  | cut -d '"' -f 4)"

if [ -z "$DOWNLOAD_URL" ]; then
  echo "Error: Could not find release asset '$ASSET'"
  exit 1
fi

# Create install directory
mkdir -p "$INSTALL_DIR"

# Download and install
echo "Downloading from $DOWNLOAD_URL..."
curl -fsSL "$DOWNLOAD_URL" -o "$INSTALL_DIR/zero-drift-chat"
chmod +x "$INSTALL_DIR/zero-drift-chat"

echo ""
echo "Installed to $INSTALL_DIR/zero-drift-chat"

# Check if install dir is in PATH
if ! echo "$PATH" | tr ':' '\n' | grep -q "$INSTALL_DIR"; then
  echo ""
  echo "Add to your PATH by adding this to your shell profile:"
  echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
fi

echo ""
echo "Run with: zero-drift-chat"
