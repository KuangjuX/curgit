#!/usr/bin/env bash
set -euo pipefail

# curgit installer
# Usage: curl -fsSL https://raw.githubusercontent.com/<user>/curgit/main/install.sh | bash

REPO="curgit"
INSTALL_DIR="${CURGIT_INSTALL_DIR:-/usr/local/bin}"
BOLD='\033[1m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
RESET='\033[0m'

info()    { echo -e "${CYAN}ℹ${RESET}  $1"; }
success() { echo -e "${GREEN}✔${RESET}  $1"; }
warn()    { echo -e "${YELLOW}⚠${RESET}  $1"; }
error()   { echo -e "${RED}✖${RESET}  $1" >&2; }

echo -e "\n${BOLD}curgit installer${RESET}\n"

# --- Check prerequisites ---

if ! command -v cargo &>/dev/null; then
    error "Rust toolchain not found."
    info "Install Rust first: ${BOLD}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${RESET}"
    exit 1
fi

if ! command -v git &>/dev/null; then
    error "git is not installed."
    exit 1
fi

# --- Determine source ---

BUILD_DIR=""
CLEANUP=false

if [ -f "Cargo.toml" ] && grep -q 'name = "curgit"' Cargo.toml 2>/dev/null; then
    info "Building from local source..."
    BUILD_DIR="$(pwd)"
else
    info "Cloning curgit repository..."
    BUILD_DIR="$(mktemp -d)"
    CLEANUP=true
    git clone --depth 1 https://github.com/curgit/curgit.git "$BUILD_DIR" 2>/dev/null || {
        error "Failed to clone repository. Building from local source if available."
        if [ "$CLEANUP" = true ]; then rm -rf "$BUILD_DIR"; fi
        exit 1
    }
fi

# --- Build ---

info "Building release binary (this may take a minute)..."
cd "$BUILD_DIR"
cargo build --release 2>&1 | tail -3

BINARY="$BUILD_DIR/target/release/curgit"

if [ ! -f "$BINARY" ]; then
    error "Build failed — binary not found at $BINARY"
    if [ "$CLEANUP" = true ]; then rm -rf "$BUILD_DIR"; fi
    exit 1
fi

# --- Install ---

if [ -w "$INSTALL_DIR" ]; then
    cp "$BINARY" "$INSTALL_DIR/curgit"
else
    warn "Need sudo to install to $INSTALL_DIR"
    sudo cp "$BINARY" "$INSTALL_DIR/curgit"
fi

chmod +x "$INSTALL_DIR/curgit"

# --- Cleanup ---

if [ "$CLEANUP" = true ]; then
    rm -rf "$BUILD_DIR"
fi

# --- Verify ---

if command -v curgit &>/dev/null; then
    VERSION=$(curgit --version 2>/dev/null || echo "unknown")
    success "curgit installed successfully! ($VERSION)"
    info "Installed to: ${BOLD}$INSTALL_DIR/curgit${RESET}"
else
    success "Binary installed to $INSTALL_DIR/curgit"
    warn "$INSTALL_DIR may not be in your PATH."
    info "Add it with: ${BOLD}export PATH=\"$INSTALL_DIR:\$PATH\"${RESET}"
fi

echo -e "\n${BOLD}Quick start:${RESET}"
echo "  git add ."
echo "  curgit"
echo ""
