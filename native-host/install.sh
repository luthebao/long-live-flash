#!/usr/bin/env bash
# Install the Llflash RTMP native messaging host for Chrome/Chromium/Edge/Brave/Firefox.
#
# Usage:
#   ./install.sh [browser]
#     browser ∈ {chrome, chromium, edge, brave, firefox, all}   (default: chrome)
#
# The script:
#   1. Resolves the absolute path to the built llflash-rtmp-host binary.
#   2. Writes a per-user manifest JSON pointing at that binary.
#   3. Drops it in the right NativeMessagingHosts directory for the chosen
#      browser, creating the directory if needed.
#
# The extension ID is fixed (pinned via the `key` field in manifest.json5).
# The host name is hard-coded to "com.longliveflash.rtmp_host" because the
# wasm bridge inside the extension hard-codes that string in its
# `runtime.connectNative` call. Don't rename without updating both ends.

set -euo pipefail

HOST_NAME="com.longliveflash.rtmp_host"
BROWSER="${1:-chrome}"

# Locate the built binary. Two layouts are supported:
#   1. Packaged release tarball: binary is a sibling of this script.
#   2. Dev workspace: binary lives in <repo>/target/{release,debug}/.
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &> /dev/null && pwd)"
WORKSPACE_ROOT="$(cd -- "$SCRIPT_DIR/.." &> /dev/null && pwd)"
BUNDLED_BIN="$SCRIPT_DIR/llflash-rtmp-host"
RELEASE_BIN="$WORKSPACE_ROOT/target/release/llflash-rtmp-host"
DEBUG_BIN="$WORKSPACE_ROOT/target/debug/llflash-rtmp-host"

if [[ -x "$BUNDLED_BIN" ]]; then
    BINARY="$BUNDLED_BIN"
elif [[ -x "$RELEASE_BIN" ]]; then
    BINARY="$RELEASE_BIN"
elif [[ -x "$DEBUG_BIN" ]]; then
    BINARY="$DEBUG_BIN"
else
    echo "ERROR: llflash-rtmp-host binary not found." >&2
    echo "       Looked for:" >&2
    echo "         $BUNDLED_BIN" >&2
    echo "         $RELEASE_BIN" >&2
    echo "         $DEBUG_BIN" >&2
    echo "       Build it first: cargo build --release -p llflash_rtmp_host" >&2
    exit 1
fi

echo "binary:    $BINARY"
echo "host name: $HOST_NAME"

TEMPLATE="$SCRIPT_DIR/manifest/com.longliveflash.rtmp_host.template.json"
if [[ ! -f "$TEMPLATE" ]]; then
    echo "ERROR: template not found at $TEMPLATE" >&2
    exit 1
fi

# Render manifest. Use the manifest's own JSON-escape rules for the binary
# path; macOS paths shouldn't need escaping, but doing it lets the script
# survive paths with quotes.
ESCAPED_BIN="$(python3 -c 'import json,sys; print(json.dumps(sys.argv[1]).strip(chr(34)))' "$BINARY")"
MANIFEST_JSON="$(sed -e "s|__BINARY_PATH__|$ESCAPED_BIN|" "$TEMPLATE")"

install_one() {
    local label="$1"
    local dir="$2"
    mkdir -p "$dir"
    local out="$dir/$HOST_NAME.json"
    printf '%s\n' "$MANIFEST_JSON" > "$out"
    chmod 644 "$out"
    echo "[$label] -> $out"
}

OS="$(uname -s)"
case "$OS" in
    Darwin)
        CHROME_DIR="$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts"
        CHROMIUM_DIR="$HOME/Library/Application Support/Chromium/NativeMessagingHosts"
        EDGE_DIR="$HOME/Library/Application Support/Microsoft Edge/NativeMessagingHosts"
        BRAVE_DIR="$HOME/Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts"
        FIREFOX_DIR="$HOME/Library/Application Support/Mozilla/NativeMessagingHosts"
        ;;
    Linux)
        CHROME_DIR="$HOME/.config/google-chrome/NativeMessagingHosts"
        CHROMIUM_DIR="$HOME/.config/chromium/NativeMessagingHosts"
        EDGE_DIR="$HOME/.config/microsoft-edge/NativeMessagingHosts"
        BRAVE_DIR="$HOME/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts"
        FIREFOX_DIR="$HOME/.mozilla/native-messaging-hosts"
        ;;
    *)
        echo "ERROR: this script only handles macOS and Linux." >&2
        echo "       On Windows, run: PowerShell -ExecutionPolicy Bypass -File native-host/install.ps1 [browser]" >&2
        exit 1
        ;;
esac

case "$BROWSER" in
    chrome)   install_one "chrome"   "$CHROME_DIR" ;;
    chromium) install_one "chromium" "$CHROMIUM_DIR" ;;
    edge)     install_one "edge"     "$EDGE_DIR" ;;
    brave)    install_one "brave"    "$BRAVE_DIR" ;;
    firefox)  install_one "firefox"  "$FIREFOX_DIR" ;;
    all)
        install_one "chrome"   "$CHROME_DIR"
        install_one "chromium" "$CHROMIUM_DIR"
        install_one "edge"     "$EDGE_DIR"
        install_one "brave"    "$BRAVE_DIR"
        install_one "firefox"  "$FIREFOX_DIR"
        ;;
    *)
        echo "ERROR: unknown browser '$BROWSER'. Use chrome|chromium|edge|brave|firefox|all." >&2
        exit 1
        ;;
esac

echo ""
echo "Done. Restart the browser if it was already running."
