#!/usr/bin/env bash
# Install the Llflash RTMP native messaging host for Chrome/Chromium/Edge/Brave/Firefox.
#
# Three modes — picked automatically:
#   1. Bootstrap (piped from curl): downloads the latest release tarball from
#      GitHub and installs it under ~/.llflash/rtmp-host. Use as:
#        curl -fsSL https://github.com/luthebao/luthebao/releases/download/llflash/rtmp-host-install.sh | bash
#   2. Local-tarball: when the binary sits next to this script (extracted
#      release tarball), uses that binary directly — no network.
#   3. Dev workspace: when run from the source tree, uses
#      ../target/{release,debug}/.
#
# Usage:
#   ./install.sh [browser]
#     browser ∈ {chrome, chromium, edge, brave, firefox, all}   (default: chrome)
#
# Env overrides:
#   LLFLASH_REPO         GitHub owner/repo to download from
#                        (default: luthebao/luthebao)
#   LLFLASH_TAG          Release tag (default: llflash)
#   LLFLASH_INSTALL_DIR  Where to extract the binary in bootstrap mode
#                        (default: ~/.llflash/rtmp-host)
#
# The host name is hard-coded to "com.longliveflash.rtmp_host" because the
# wasm bridge inside the extension hard-codes that string in its
# `runtime.connectNative` call. Don't rename without updating both ends.

set -euo pipefail

LLFLASH_REPO="${LLFLASH_REPO:-luthebao/luthebao}"
LLFLASH_TAG="${LLFLASH_TAG:-llflash}"
LLFLASH_INSTALL_DIR="${LLFLASH_INSTALL_DIR:-$HOME/.llflash/rtmp-host}"

HOST_NAME="com.longliveflash.rtmp_host"
BROWSER="${1:-chrome}"

# ---- Detect OS + arch -----------------------------------------------------
case "$(uname -s)" in
    Darwin) OS_TAG="macOS" ;;
    Linux)  OS_TAG="Linux" ;;
    *)
        echo "ERROR: this script only handles macOS and Linux." >&2
        echo "       On Windows: irm https://github.com/$LLFLASH_REPO/releases/download/$LLFLASH_TAG/rtmp-host-install.ps1 | iex" >&2
        exit 1
        ;;
esac
case "$(uname -m)" in
    arm64|aarch64) ARCH_TAG="arm64" ;;
    x86_64|amd64)  ARCH_TAG="x64" ;;
    *) echo "ERROR: unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

# Workflow currently only ships macOS-arm64 and Linux-x64.
case "$OS_TAG-$ARCH_TAG" in
    macOS-arm64|Linux-x64) ;;
    *)
        echo "ERROR: no released binary for $OS_TAG-$ARCH_TAG." >&2
        echo "       Supported: macOS-arm64, Linux-x64, Windows-x64 (via .ps1)." >&2
        exit 1
        ;;
esac

# ---- Resolve binary location ----------------------------------------------
# BASH_SOURCE[0] is unset/non-file when piped from curl — that's how we detect
# bootstrap mode vs a script invoked from disk.
SCRIPT_DIR=""
if [[ -n "${BASH_SOURCE[0]:-}" && -f "${BASH_SOURCE[0]}" ]]; then
    SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &> /dev/null && pwd)"
fi

BINARY=""
TEMPLATE_PATH=""

try_local_layouts() {
    if [[ -z "$SCRIPT_DIR" ]]; then
        return 1
    fi
    local sibling="$SCRIPT_DIR/llflash-rtmp-host"
    local workspace_root release_bin debug_bin
    workspace_root="$(cd -- "$SCRIPT_DIR/.." &> /dev/null && pwd)"
    release_bin="$workspace_root/target/release/llflash-rtmp-host"
    debug_bin="$workspace_root/target/debug/llflash-rtmp-host"

    if [[ -x "$sibling" ]]; then
        BINARY="$sibling"
        TEMPLATE_PATH="$SCRIPT_DIR/manifest/$HOST_NAME.template.json"
    elif [[ -x "$release_bin" ]]; then
        BINARY="$release_bin"
        TEMPLATE_PATH="$SCRIPT_DIR/manifest/$HOST_NAME.template.json"
    elif [[ -x "$debug_bin" ]]; then
        BINARY="$debug_bin"
        TEMPLATE_PATH="$SCRIPT_DIR/manifest/$HOST_NAME.template.json"
    else
        return 1
    fi
    [[ -f "$TEMPLATE_PATH" ]] || return 1
    return 0
}

bootstrap_download() {
    local api_url release_json asset_url tmp_dir
    api_url="https://api.github.com/repos/$LLFLASH_REPO/releases/tags/$LLFLASH_TAG"
    echo "Fetching release '$LLFLASH_TAG' from $LLFLASH_REPO..."

    release_json="$(curl -fsSL -H 'Accept: application/vnd.github+json' "$api_url")" || {
        echo "ERROR: failed to fetch $api_url" >&2
        exit 1
    }

    # Pick the first browser_download_url whose value ends with the OS/arch
    # suffix we need. Assets at one tag belong to one release, so there's
    # exactly one match per OS-arch.
    asset_url="$(printf '%s' "$release_json" \
        | grep -Eo "\"browser_download_url\": *\"[^\"]*llflash-rtmp-host-[^\"]*-${OS_TAG}-${ARCH_TAG}\\.tar\\.gz\"" \
        | head -1 \
        | sed -E 's/.*"(https:[^"]*)"/\1/')"

    if [[ -z "$asset_url" ]]; then
        echo "ERROR: no llflash-rtmp-host-*-${OS_TAG}-${ARCH_TAG}.tar.gz asset on $LLFLASH_REPO@$LLFLASH_TAG" >&2
        exit 1
    fi

    echo "Downloading $asset_url"
    tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/llflash-rtmp-host.XXXXXX")"
    trap "rm -rf '$tmp_dir'" EXIT
    curl -fsSL "$asset_url" -o "$tmp_dir/release.tar.gz"

    mkdir -p "$LLFLASH_INSTALL_DIR"
    # --strip-components=1 drops the tarball's top-level "llflash-rtmp-host/"
    # so files land directly in $LLFLASH_INSTALL_DIR.
    tar -xzf "$tmp_dir/release.tar.gz" -C "$LLFLASH_INSTALL_DIR" --strip-components=1

    BINARY="$LLFLASH_INSTALL_DIR/llflash-rtmp-host"
    TEMPLATE_PATH="$LLFLASH_INSTALL_DIR/manifest/$HOST_NAME.template.json"
    chmod +x "$BINARY"
}

if ! try_local_layouts; then
    bootstrap_download
fi

if [[ ! -x "$BINARY" ]]; then
    echo "ERROR: binary at $BINARY isn't executable." >&2
    exit 1
fi
if [[ ! -f "$TEMPLATE_PATH" ]]; then
    echo "ERROR: manifest template not found at $TEMPLATE_PATH" >&2
    exit 1
fi

echo "binary:    $BINARY"
echo "host name: $HOST_NAME"

# ---- Render manifest ------------------------------------------------------
# Use python's JSON encoder to escape the binary path. macOS paths usually
# don't need it, but it survives quotes/backslashes in $HOME if any user
# ever has them.
ESCAPED_BIN="$(python3 -c 'import json,sys; print(json.dumps(sys.argv[1]).strip(chr(34)))' "$BINARY")"
MANIFEST_JSON="$(sed -e "s|__BINARY_PATH__|$ESCAPED_BIN|" "$TEMPLATE_PATH")"

# ---- Install for selected browser(s) --------------------------------------
install_one() {
    local label="$1"
    local dir="$2"
    mkdir -p "$dir"
    local out="$dir/$HOST_NAME.json"
    printf '%s\n' "$MANIFEST_JSON" > "$out"
    chmod 644 "$out"
    echo "[$label] -> $out"
}

case "$OS_TAG" in
    macOS)
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
