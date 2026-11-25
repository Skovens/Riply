#!/usr/bin/env bash
# install-makemkv.sh — install MakeMKV for Linux so Riply can rip.
#
# MakeMKV has no packaged Linux install. It ships as two tarballs:
#   * makemkv-bin-<V>.tar.gz — a precompiled `makemkvcon` (per-arch) + data
#   * makemkv-oss-<V>.tar.gz — source that builds the shared libraries
#     (libmakemkv, libdriveio, libmmbd) that makemkvcon links against
#
# Neither tarball is self-contained: makemkvcon won't run until the oss
# libraries are compiled and installed. This script does the whole job:
# it finds the latest version, downloads both tarballs, verifies their
# SHA256 against MakeMKV's published hash file, installs the build
# dependencies, compiles the libraries from source, installs everything,
# and finally runs `makemkvcon` to confirm the install works.
#
# It asks for sudo exactly once (for apt-get + the installs). If you would
# rather install by hand, answer "no" at the Riply prompt — it will print
# the same steps with direct links — or run this script's --help.
#
# Flags:
#   --version <V>   Install a specific version (default: latest from history.html)
#   --no-deps       Skip the apt-get dependency step (deps already installed)
#   --prefix <P>    Install prefix (default: /usr)
#   --help          Show this help

set -euo pipefail

# ---- defaults -------------------------------------------------------------
DOWNLOAD_BASE="https://www.makemkv.com/download"
VERSION=""
INSTALL_DEPS=1
PREFIX="/usr"
KEEP_TMP=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --no-deps) INSTALL_DEPS=0; shift ;;
    --prefix) PREFIX="$2"; shift 2 ;;
    --keep-tmp) KEEP_TMP=1; shift ;;
    --help|-h) sed -n '2,30p' "$0"; exit 0 ;;
    *) echo "Unknown flag: $1" >&2; exit 2 ;;
  esac
done

log()  { printf '\033[1;36m==> %s\033[0m\n' "$*"; }
warn() { printf '\033[1;33m!!  %s\033[0m\n' "$*" >&2; }
die()  { printf '\033[1;31mERR %s\033[0m\n' "$*" >&2; exit 1; }

command -v curl    >/dev/null 2>&1 || die "curl is required"
command -v tar     >/dev/null 2>&1 || die "tar is required"
command -v make    >/dev/null 2>&1 || die "make is required (install build-essential)"

# ---- architecture ----------------------------------------------------------
# MakeMKV's bin tarball names its arch dirs; map uname -m to them.
arch_dir() {
  case "$(uname -m)" in
    x86_64|amd64)   echo amd64 ;;
    aarch64|arm64)  echo arm64 ;;
    armv7l|armv8l)  echo armhf ;;
    i686|i586)      echo i386 ;;
    *) die "unsupported architecture: $(uname -m) — install MakeMKV by hand: $(printf '%s/makemkv-oss-%%s.tar.gz' "$DOWNLOAD_BASE")" ;;
  esac
}
ARCH="$(arch_dir)"

# ---- version ---------------------------------------------------------------
latest_version() {
  # history.html lists "MakeMKV v2.0.0 (date)" per release, newest first.
  curl -fsSL --max-time 30 "${DOWNLOAD_BASE}/history.html" \
    | grep -oE 'MakeMKV v[0-9]+\.[0-9]+\.[0-9]+' \
    | head -1 \
    | sed 's/.*v//'
}
if [[ -z "$VERSION" ]]; then
  log "Detecting latest MakeMKV version"
  VERSION="$(latest_version)" || die "could not detect the latest version"
  [[ -n "$VERSION" ]] || die "could not detect the latest version"
fi
log "Using MakeMKV v${VERSION}"

BIN_TAR="makemkv-bin-${VERSION}.tar.gz"
OSS_TAR="makemkv-oss-${VERSION}.tar.gz"
SHA_TXT="makemkv-sha-${VERSION}.txt"

# ---- workspace -------------------------------------------------------------
WORK="$(mktemp -d /tmp/makemkv-install.XXXXXX)"
trap 'if [[ "$KEEP_TMP" != 1 ]]; then rm -rf "$WORK"; fi' EXIT

log "Downloading tarballs to $WORK"
curl -fsSL --max-time 600 -o "$WORK/$BIN_TAR" "${DOWNLOAD_BASE}/${BIN_TAR}"
curl -fsSL --max-time 600 -o "$WORK/$OSS_TAR" "${DOWNLOAD_BASE}/${OSS_TAR}"

# ---- integrity check -------------------------------------------------------
# Best-effort: a missing/unreadable sha file is a warning (the install can
# still proceed); a hash mismatch is fatal (never install a bad binary).
log "Verifying SHA256"
if curl -fsSL --max-time 30 -o "$WORK/${SHA_TXT}" "${DOWNLOAD_BASE}/${SHA_TXT}"; then
  for name in "$BIN_TAR" "$OSS_TAR"; do
    sha_re="^[0-9a-f]{64} +${name}$"
    want="$(grep -oE "$sha_re" "$WORK/$SHA_TXT" | awk '{print $1}' || true)"
    got="$(cd "$WORK" && sha256sum "$name" | awk '{print $1}')"
    if [[ -z "$want" ]]; then
      warn "no published hash for $name — skipping verification"
    elif [[ "$want" != "$got" ]]; then
      die "SHA256 MISMATCH for $name (want $want, got $got) — refusing to install"
    else
      log "  verified $name"
    fi
  done
else
  warn "could not fetch ${SHA_TXT} — skipping hash verification"
fi

# ---- unpack ----------------------------------------------------------------
log "Unpacking"
tar -xzf "$WORK/$BIN_TAR" -C "$WORK"
tar -xzf "$WORK/$OSS_TAR" -C "$WORK"
BIN_DIR="$WORK/makemkv-bin-${VERSION}"
OSS_DIR="$WORK/makemkv-oss-${VERSION}"
[[ -d "$BIN_DIR" ]] || die "expected dir $BIN_DIR not found after unpack"
[[ -d "$OSS_DIR" ]] || die "expected dir $OSS_DIR not found after unpack"

# ---- sudo ------------------------------------------------------------------
SUDO=""
if [[ "$(id -u)" != 0 ]]; then
  if command -v sudo >/dev/null 2>&1; then
    SUDO="sudo"
  else
    die "need root or sudo to install; re-run as root, or use --no-deps and install manually"
  fi
fi

# ---- build dependencies ----------------------------------------------------
# makemkvcon (bin) needs no deps; the oss libraries need a C/C++ toolchain
# plus headers. The GUI is disabled, so Qt/GL packages are not required.
if [[ "$INSTALL_DEPS" == 1 ]]; then
  log "Installing build dependencies (this is the sudo password prompt)"
  if command -v apt-get >/dev/null 2>&1; then
    $SUDO apt-get update -qq
    $SUDO apt-get install -y --no-install-recommends \
      build-essential pkg-config \
      zlib1g-dev libexpat1-dev libssl-dev \
      libavcodec-dev libavutil-dev
  else
    warn "apt-get not found — install these build dependencies yourself, then re-run with --no-deps:"
    warn "  gcc g++ make pkg-config + dev headers for: zlib, expat, openssl, libavcodec"
    warn "  (Debian/Ubuntu: build-essential pkg-config zlib1g-dev libexpat1-dev libssl-dev libavcodec-dev)"
    INSTALL_DEPS=0
  fi
fi

# ---- build + install the oss libraries --------------------------------------
# The oss source compiles libmakemkv/libdriveio/libmmbd into $(PREFIX)/lib
# and runs ldconfig itself. --disable-gui/--disable-qt5 skip the Qt5 GUI,
# which is what makes the dependency list small.
log "Building MakeMKV libraries from source (this is the slow part)"
cd "$OSS_DIR"
./configure --prefix="$PREFIX" --disable-gui --disable-qt5 >/dev/null
make -j"$(nproc 2>/dev/null || echo 2)" >/dev/null
$SUDO make install

# ---- install the precompiled binary + data ----------------------------------
# Mirrors the bin Makefile's install target, but without its interactive EULA
# `make` step — you already accepted at the Riply prompt, so we install the
# files directly. This is non-interactive and idempotent.
log "Installing makemkvcon"
$SUDO install -d "$PREFIX/share/MakeMKV" "$PREFIX/bin"
$SUDO install -t "$PREFIX/bin" "$BIN_DIR/bin/$ARCH/makemkvcon"
$SUDO install -m 644 -t "$PREFIX/share/MakeMKV" \
  "$BIN_DIR/src/share/appdata.tar" \
  "$BIN_DIR/src/share/blues.jar" \
  "$BIN_DIR/src/share/blues.policy"
$SUDO ln -s -f makemkvcon "$PREFIX/bin/sdftool"

# ---- verify ----------------------------------------------------------------
log "Verifying install"
if command -v makemkvcon >/dev/null 2>&1; then
  # disc:0 is a harmless probe; we only need makemkvcon to start, run, and
  # report its version (MSG:1005) — a missing-disc error still proves the
  # binary + libraries load.
  if makemkvcon -r info disc:0 2>&1 | grep -q 'MSG:1005'; then
    ver="$(makemkvcon -r info disc:0 2>&1 | grep -oE 'MakeMKV v[0-9.]+[^"]*' | head -1 || true)"
    log "SUCCESS — $ver is installed and on your PATH."
    log "Riply detects MakeMKV automatically; no restart is needed."
  else
    warn "makemkvcon ran but did not print a version banner."
    warn "Re-run: makemkvcon -r info disc:0"
    exit 1
  fi
else
  warn "makemkvcon not found on PATH after install."
  warn "Add $PREFIX/bin to PATH, or re-run with --prefix matching your PATH."
  exit 1
fi
