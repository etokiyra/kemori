#!/bin/sh
# Kemori installer — downloads a prebuilt, checksum-verified release binary.
#
#   curl -fsSL https://kiyra.lol/kemori/install.sh | sh
#
# Options (environment variables):
#   KEMORI_VERSION       install a specific version, e.g. 3.2.1 (default: latest)
#   KEMORI_INSTALL_DIR   install here instead of ~/.local/bin
#
# The script never elevates privileges and never edits shell startup files.
set -eu

REPO="${KEMORI_REPO:-etokiyra/kemori}"
BIN="kemori"

say() { printf '%s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

need() {
    command -v "$1" >/dev/null 2>&1 || die "this installer needs '$1' on your PATH"
}

# --- platform -------------------------------------------------------------

os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
    Linux)  platform="linux" ;;
    Darwin) platform="macos" ;;
    *)      die "unsupported operating system: $os (Kemori supports Linux and macOS)" ;;
esac

case "$arch" in
    x86_64|amd64)  cpu="x86_64" ;;
    aarch64|arm64) cpu="arm64" ;;
    *)             die "unsupported architecture: $arch" ;;
esac

# Candidate builds, best first. The glibc build is linked against the glibc of
# the machine that built it, so it refuses to start on older distributions; the
# static musl build runs anywhere but cannot use the glibc-only managed bundles
# (clangd, asm-lsp). Try the native build, then fall back.
if [ "$platform" = "macos" ]; then
    candidates="macos-$cpu"
elif [ "$cpu" = "x86_64" ]; then
    if ldd --version 2>/dev/null | grep -qi musl; then
        candidates="linux-x86_64-musl"
    else
        candidates="linux-x86_64-gnu linux-x86_64-musl"
    fi
else
    die "no prebuilt Linux $cpu release; build from source (see README)"
fi

# --- tools ----------------------------------------------------------------

if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -q "$1" -O "$2"; }
else
    die "this installer needs 'curl' or 'wget'"
fi

need tar
need uname

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    elif command -v openssl >/dev/null 2>&1; then
        openssl dgst -sha256 "$1" | sed 's/.*= //'
    else
        die "no SHA-256 tool found (need sha256sum, shasum or openssl)"
    fi
}

# --- version --------------------------------------------------------------

version="${KEMORI_VERSION:-}"
if [ -z "$version" ]; then
    say "→ looking up the latest release…"
    fetch "https://api.github.com/repos/$REPO/releases/latest" /tmp/kemori-release.$$ 2>/dev/null \
        || die "could not reach the GitHub API"
    version="$(sed -n 's/.*"tag_name": *"v\{0,1\}\([^"]*\)".*/\1/p' /tmp/kemori-release.$$ | head -n1)"
    rm -f /tmp/kemori-release.$$
    [ -n "$version" ] || die "could not determine the latest version; set KEMORI_VERSION"
fi

base="https://github.com/$REPO/releases/download/v$version"

# --- download and verify --------------------------------------------------

tmp="$(mktemp -d "${TMPDIR:-/tmp}/kemori-install.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT INT TERM

fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || die "could not download SHA256SUMS"

# A glibc build that cannot start is worse than a portable one that can: the
# loader fails before main() with "version `GLIBC_x.y' not found". Ask the
# loader whether it can run the extracted binary and move on if it cannot.
can_run() {
    command -v ldd >/dev/null 2>&1 || return 0
    ! ldd "$1" 2>&1 | grep -q "not found"
}

unpacked=""
for asset_platform in $candidates; do
    archive="$BIN-$version-$asset_platform.tar.gz"

    say "→ downloading $archive"
    if ! fetch "$base/$archive" "$tmp/$archive"; then
        say "  download failed; trying the next build"
        continue
    fi

    expected="$(sed -n "s/^\([0-9a-f]\{64\}\)  *$archive$/\1/p" "$tmp/SHA256SUMS" | head -n1)"
    [ -n "$expected" ] || die "no checksum published for $archive"

    actual="$(sha256_of "$tmp/$archive")"
    [ "$expected" = "$actual" ] || die "checksum mismatch for $archive"
    say "→ checksum verified"

    rm -rf "$tmp/unpack"
    mkdir -p "$tmp/unpack"
    tar -xzf "$tmp/$archive" -C "$tmp/unpack" "$BIN" || die "could not unpack $archive"

    if can_run "$tmp/unpack/$BIN"; then
        unpacked="$tmp/unpack/$BIN"
        break
    fi
    say "  this build needs a newer glibc; trying the portable build"
done

[ -n "$unpacked" ] || die "no prebuilt build runs on this system; build from source (see README)"

# --- install --------------------------------------------------------------
#
# Install where the shell will actually find the binary. A `curl | sh` install
# cannot ask for a sudo password safely (stdin is the script itself), so a
# system directory is used only when it is already writable — root, or a system
# that made /usr/local/bin writable. Otherwise prefer a user directory that is
# already on PATH: putting a binary somewhere the shell cannot see is a broken
# install, and editing someone's shell startup files is their decision, not
# this script's.

on_path() {
    case ":$PATH:" in
        *":$1:"*) return 0 ;;
        *) return 1 ;;
    esac
}

usable_dir() {
    [ -d "$1" ] || mkdir -p "$1" 2>/dev/null || return 1
    [ -w "$1" ]
}

unreachable_dir=0
if [ -n "${KEMORI_INSTALL_DIR:-}" ]; then
    dir="$KEMORI_INSTALL_DIR"
    usable_dir "$dir" || die "cannot write to KEMORI_INSTALL_DIR ($dir)"
elif usable_dir /usr/local/bin; then
    dir="/usr/local/bin"
else
    dir=""
    for candidate in "$HOME/.local/bin" "$HOME/bin" "$HOME/.bin"; do
        if on_path "$candidate" && usable_dir "$candidate"; then
            dir="$candidate"
            break
        fi
    done
    if [ -z "$dir" ]; then
        dir="$HOME/.local/bin"
        usable_dir "$dir" || die "could not create $dir"
        unreachable_dir=1
    fi
fi

install -m 755 "$unpacked" "$dir/$BIN" 2>/dev/null \
    || { cp "$unpacked" "$dir/$BIN" && chmod 755 "$dir/$BIN"; } \
    || die "could not write to $dir"

say "→ installed $BIN $version to $dir/$BIN"

if [ "$unreachable_dir" = 1 ]; then
    say ""
    say "  No writable directory on your PATH was found."
    say "  Add this directory to your PATH with:"
    say ""
    say "    echo 'export PATH=\"$dir:\$PATH\"' >> ~/.profile"
    say ""
else
    say "  Run it with:  $BIN ."
fi
