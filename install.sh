#!/bin/bash
# ForgeCode Installer for Linux/macOS
# Usage: curl -fsSL https://raw.githubusercontent.com/BbangMxn/ForgeCode/main/install.sh | bash

set -e

echo "🔧 ForgeCode Installer"
echo ""

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
    linux)
        case "$ARCH" in
            x86_64) TARGET="x86_64-unknown-linux-gnu" ;;
            aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
            *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        ;;
    darwin)
        case "$ARCH" in
            x86_64) TARGET="x86_64-apple-darwin" ;;
            arm64) TARGET="aarch64-apple-darwin" ;;
            *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
        esac
        ;;
    *)
        echo "Unsupported OS: $OS"
        exit 1
        ;;
esac

REPO="BbangMxn/ForgeCode"

# Get latest release
echo "Fetching latest release..."
RELEASE_URL="https://api.github.com/repos/$REPO/releases/latest"
RELEASE_JSON=$(curl -fsSL "$RELEASE_URL")
VERSION=$(echo "$RELEASE_JSON" | grep -o '"tag_name": "[^"]*' | cut -d'"' -f4)

echo "Latest version: $VERSION"

# Find download URL
DOWNLOAD_URL=$(echo "$RELEASE_JSON" | grep -o "\"browser_download_url\": \"[^\"]*$TARGET[^\"]*\"" | cut -d'"' -f4)

if [ -z "$DOWNLOAD_URL" ]; then
    echo "Error: No release found for $TARGET"
    exit 1
fi

# Download and extract
TEMP_DIR=$(mktemp -d)
ARCHIVE_NAME=$(basename "$DOWNLOAD_URL")

echo "Downloading $ARCHIVE_NAME..."
curl -fsSL "$DOWNLOAD_URL" -o "$TEMP_DIR/$ARCHIVE_NAME"

echo "Extracting..."
cd "$TEMP_DIR"
tar -xzf "$ARCHIVE_NAME"

# Install
INSTALL_DIR="$HOME/.forge/bin"
mkdir -p "$INSTALL_DIR"
cp forge "$INSTALL_DIR/"
chmod +x "$INSTALL_DIR/forge"

# Add to PATH
SHELL_RC=""
if [ -n "$ZSH_VERSION" ]; then
    SHELL_RC="$HOME/.zshrc"
elif [ -n "$BASH_VERSION" ]; then
    SHELL_RC="$HOME/.bashrc"
fi

if [ -n "$SHELL_RC" ] && ! grep -q ".forge/bin" "$SHELL_RC" 2>/dev/null; then
    echo 'export PATH="$HOME/.forge/bin:$PATH"' >> "$SHELL_RC"
    echo "Added ~/.forge/bin to PATH in $SHELL_RC"
fi

# Cleanup
rm -rf "$TEMP_DIR"

echo ""
echo "✓ ForgeCode $VERSION installed successfully!"
echo ""
echo "Installation path: $INSTALL_DIR/forge"
echo ""
echo "To get started:"
echo "  1. Restart your terminal (or run: source $SHELL_RC)"
echo "  2. Run: forge"
echo ""
