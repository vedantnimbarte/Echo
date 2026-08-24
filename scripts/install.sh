#!/bin/sh
# Echo installer for macOS and Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/vedantnimbarte/Echo/main/scripts/install.sh | sh
#
# Environment:
#   ECHO_VERSION   install a specific tag (e.g. v0.1.0) instead of the latest
#   ECHO_INSTALL_DIR  Linux only — where to put the AppImage (default ~/.local/bin)
#
# POSIX sh on purpose: this has to run under dash, ash and busybox, not just
# bash. No jq dependency either — a fresh machine won't have it.

set -eu

REPO="vedantnimbarte/Echo"
INSTALL_DIR="${ECHO_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf '%s\n' "$*"; }
warn() { printf '\033[33m!\033[0m %s\n' "$*" >&2; }
die() {
    printf '\033[31merror:\033[0m %s\n' "$*" >&2
    exit 1
}

need() {
    command -v "$1" >/dev/null 2>&1 || die "'$1' is required but not installed."
}

need curl

# ── Work out what to download ────────────────────────────────────────────────

os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
Darwin) platform="macos" ;;
Linux) platform="linux" ;;
*) die "Unsupported OS '$os'. Echo ships installers for macOS, Linux and Windows." ;;
esac

if [ "$platform" = "linux" ]; then
    case "$arch" in
    x86_64 | amd64) : ;;
    *) die "Unsupported architecture '$arch'. Linux builds are x86_64 only for now; build from source instead — see the README." ;;
    esac
fi

if [ -n "${ECHO_VERSION:-}" ]; then
    api="https://api.github.com/repos/$REPO/releases/tags/$ECHO_VERSION"
else
    api="https://api.github.com/repos/$REPO/releases/latest"
fi

release_json="$(curl -fsSL "$api" 2>/dev/null)" || die "Couldn't reach the GitHub releases API. Are you online?"

# No jq: pull download URLs straight out of the JSON.
asset_url() {
    printf '%s' "$release_json" |
        grep '"browser_download_url"' |
        sed -E 's/.*"browser_download_url": *"([^"]+)".*/\1/' |
        grep -E "$1" |
        head -n 1
}

tag="$(printf '%s' "$release_json" | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/' | head -n 1)"
[ -n "$tag" ] || die "No published release found for $REPO. If you're expecting one, it may still be a draft."

if [ "$platform" = "macos" ]; then
    url="$(asset_url '\.dmg$')"
    [ -n "$url" ] || die "No .dmg in release $tag."
else
    url="$(asset_url '\.AppImage$')"
    [ -n "$url" ] || die "No .AppImage in release $tag. A .deb may be available — see the release page."
fi

file="$(basename "$url")"
tmp="$(mktemp -d)"
# shellcheck disable=SC2064 # expand $tmp now, not at trap time
trap "rm -rf '$tmp'" EXIT INT TERM

say "Installing Echo $tag ($platform)"
say "  ↓ $file"
curl -fsSL --proto '=https' --tlsv1.2 -o "$tmp/$file" "$url" || die "Download failed."

# ── Verify against the release checksums, when present ───────────────────────

sums_url="$(asset_url 'SHA256SUMS\.txt$')"
if [ -n "$sums_url" ] && curl -fsSL -o "$tmp/SHA256SUMS.txt" "$sums_url" 2>/dev/null; then
    if command -v sha256sum >/dev/null 2>&1; then
        actual="$(sha256sum "$tmp/$file" | cut -d' ' -f1)"
    elif command -v shasum >/dev/null 2>&1; then
        actual="$(shasum -a 256 "$tmp/$file" | cut -d' ' -f1)"
    else
        actual=""
        warn "No sha256sum/shasum available; skipping checksum verification."
    fi

    if [ -n "$actual" ]; then
        expected="$(grep -F " $file" "$tmp/SHA256SUMS.txt" | cut -d' ' -f1 | head -n 1)"
        if [ -z "$expected" ]; then
            warn "$file is not listed in SHA256SUMS.txt; skipping verification."
        elif [ "$actual" != "$expected" ]; then
            die "Checksum mismatch for $file. Expected $expected, got $actual. Not installing."
        else
            say "  ✓ checksum verified"
        fi
    fi
else
    warn "This release publishes no SHA256SUMS.txt; the download could not be verified."
fi

# ── Install ──────────────────────────────────────────────────────────────────

if [ "$platform" = "macos" ]; then
    mount_point="$tmp/mnt"
    mkdir -p "$mount_point"
    hdiutil attach -nobrowse -quiet -mountpoint "$mount_point" "$tmp/$file" ||
        die "Couldn't mount $file."

    app="$(find "$mount_point" -maxdepth 1 -name '*.app' -print | head -n 1)"
    if [ -z "$app" ]; then
        hdiutil detach -quiet "$mount_point" || true
        die "No .app found inside $file."
    fi

    rm -rf "/Applications/$(basename "$app")"
    cp -R "$app" /Applications/ || {
        hdiutil detach -quiet "$mount_point" || true
        die "Couldn't copy into /Applications. Try again with sudo, or drag it across manually."
    }
    hdiutil detach -quiet "$mount_point" || true

    installed="/Applications/$(basename "$app")"

    # Echo is not notarised yet, so Gatekeeper would refuse to open it and offer
    # only "Move to Trash". Clearing the quarantine flag on a binary the user
    # just chose to install is what the manual right-click → Open does.
    if xattr -dr com.apple.quarantine "$installed" 2>/dev/null; then
        say "  ✓ cleared the quarantine flag (Echo isn't notarised yet)"
    else
        warn "Couldn't clear the quarantine flag. If macOS refuses to open Echo,"
        warn "right-click it in Applications and choose Open, then confirm."
    fi

    say ""
    say "Installed to $installed"
    say "Launch it from Applications, or: open -a Echo"
else
    mkdir -p "$INSTALL_DIR"
    target="$INSTALL_DIR/echo"
    mv "$tmp/$file" "$target"
    chmod +x "$target"

    say ""
    say "Installed to $target"

    case ":$PATH:" in
    *":$INSTALL_DIR:"*) say "Run it with: echo" ;;
    *)
        warn "$INSTALL_DIR is not on your PATH."
        warn "Add this to your shell profile:"
        warn "  export PATH=\"\$PATH:$INSTALL_DIR\""
        say "Or run it directly: $target"
        ;;
    esac

    say ""
    say "AppImage needs FUSE. On Debian/Ubuntu: sudo apt install libfuse2"
    say "Text injection needs xdotool (X11) or ydotool (Wayland)."
fi

say ""
say "Echo is unsigned — see https://github.com/$REPO#installing for what that means."
