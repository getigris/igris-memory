#!/bin/sh
# Igris Memory installer
# Usage: curl -fsSL https://raw.githubusercontent.com/getigris/igris-memory/main/dist/install.sh | sh
set -e

REPO="getigris/igris-memory"
INSTALL_DIR="${IGRIS_INSTALL_DIR:-/usr/local/bin}"

# ─── Detect OS and architecture ─────────────────────────────────

detect_target() {
    OS=$(uname -s)
    ARCH=$(uname -m)

    case "$OS" in
        Linux)
            case "$ARCH" in
                x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
                aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
                arm64)   TARGET="aarch64-unknown-linux-gnu" ;;
                *) echo "Error: unsupported architecture $ARCH on Linux"; exit 1 ;;
            esac
            EXT="tar.gz"
            ;;
        Darwin)
            case "$ARCH" in
                arm64)   TARGET="aarch64-apple-darwin" ;;
                aarch64) TARGET="aarch64-apple-darwin" ;;
                *) echo "Error: unsupported architecture $ARCH on macOS"; exit 1 ;;
            esac
            EXT="tar.gz"
            ;;
        *)
            echo "Error: unsupported OS $OS (use Windows binaries from GitHub Releases)"
            exit 1
            ;;
    esac
}

# ─── Get latest version ─────────────────────────────────────────

get_latest_version() {
    VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | grep '"tag_name"' \
        | head -1 \
        | sed 's/.*"tag_name": *"//;s/".*//')

    if [ -z "$VERSION" ]; then
        echo "Error: could not determine latest version"
        exit 1
    fi
}

# ─── Download and install ───────────────────────────────────────

install() {
    FILENAME="igris-memory-${TARGET}.${EXT}"
    URL="https://github.com/$REPO/releases/download/$VERSION/$FILENAME"
    CHECKSUMS_URL="https://github.com/$REPO/releases/download/$VERSION/checksums-sha256.txt"

    echo "Installing igris-memory $VERSION for $TARGET..."
    echo "  Downloading $URL"

    TMP=$(mktemp -d)
    trap 'rm -rf "$TMP"' EXIT

    curl -fsSL "$URL" -o "$TMP/$FILENAME"
    curl -fsSL "$CHECKSUMS_URL" -o "$TMP/checksums-sha256.txt"

    echo "  Verifying checksum..."
    EXPECTED=$(grep "$FILENAME" "$TMP/checksums-sha256.txt" | awk '{print $1}')
    if [ -z "$EXPECTED" ]; then
        echo "Error: checksum not found for $FILENAME"
        exit 1
    fi
    if command -v sha256sum > /dev/null 2>&1; then
        ACTUAL=$(sha256sum "$TMP/$FILENAME" | awk '{print $1}')
    elif command -v shasum > /dev/null 2>&1; then
        ACTUAL=$(shasum -a 256 "$TMP/$FILENAME" | awk '{print $1}')
    else
        echo "Warning: no sha256sum or shasum found, skipping verification"
        ACTUAL="$EXPECTED"
    fi
    if [ "$ACTUAL" != "$EXPECTED" ]; then
        echo "Error: checksum mismatch!"
        echo "  Expected: $EXPECTED"
        echo "  Got:      $ACTUAL"
        exit 1
    fi
    echo "  Checksum verified."

    echo "  Extracting..."
    tar xzf "$TMP/$FILENAME" -C "$TMP"

    echo "  Installing to $INSTALL_DIR/igmem"
    if [ -w "$INSTALL_DIR" ]; then
        mv "$TMP/igmem" "$INSTALL_DIR/igmem"
    else
        sudo mv "$TMP/igmem" "$INSTALL_DIR/igmem"
    fi
    chmod +x "$INSTALL_DIR/igmem"

    echo ""
    echo "Done! igmem $VERSION installed to $INSTALL_DIR/igmem"
    echo ""
    echo "Add to Claude Code (~/.claude/settings.json):"
    echo '  "mcpServers": {'
    echo '    "igris-memory": {'
    echo "      \"command\": \"$INSTALL_DIR/igmem\""
    echo '    }'
    echo '  }'
}

# ─── Main ────────────────────────────────────────────────────────

detect_target
get_latest_version
install
