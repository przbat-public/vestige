#!/bin/bash
set -euo pipefail

# Default to the version declared in manifest.json. A hardcoded default drifts:
# this script sat at 1.1.0 while the manifest advertised a newer bundle, so the
# "default" build silently fetched a release tag the manifest no longer named.
VERSION="${1:-$(awk -F'"' '/^[[:space:]]*"version"[[:space:]]*:/{print $4; exit}' manifest.json)}"
REPO="samvallad33/vestige"

echo "Building Vestige MCPB v${VERSION}..."

# Create server directory
mkdir -p server

# Every archive below is a distinct OS+arch build. Nothing in the MCPB manifest
# can select between them by architecture (see launch.sh), so the bundle ships
# the launcher plus the explicit win32 override and lets the launcher decide.
# `check-manifest.sh` (run at the end, and in CI) fails when a downloaded
# `vestige-mcp-*` binary is reachable from neither.

# Download macOS ARM64
echo "Downloading macOS ARM64 binary..."
curl -sfL "https://github.com/${REPO}/releases/download/v${VERSION}/vestige-mcp-aarch64-apple-darwin.tar.gz" | tar -xz -C server
mv server/vestige-mcp server/vestige-mcp-darwin-arm64
mv server/vestige server/vestige-darwin-arm64

# Download Linux x64
echo "Downloading Linux x64 binary..."
curl -sfL "https://github.com/${REPO}/releases/download/v${VERSION}/vestige-mcp-x86_64-unknown-linux-gnu.tar.gz" | tar -xz -C server
mv server/vestige-mcp server/vestige-mcp-linux-x64
mv server/vestige server/vestige-linux-x64

# Download Linux ARM64 — Graviton EC2, Raspberry Pi 4+, Apple-Silicon
# Linux VMs. Released since v3.3.0 (release.yml second-wave sweep).
echo "Downloading Linux ARM64 binary..."
curl -sfL "https://github.com/${REPO}/releases/download/v${VERSION}/vestige-mcp-aarch64-unknown-linux-gnu.tar.gz" | tar -xz -C server
mv server/vestige-mcp server/vestige-mcp-linux-arm64
mv server/vestige server/vestige-linux-arm64

# Download Windows x64
echo "Downloading Windows x64 binary..."
curl -sfL "https://github.com/${REPO}/releases/download/v${VERSION}/vestige-mcp-x86_64-pc-windows-msvc.zip" -o /tmp/win.zip
unzip -q /tmp/win.zip -d server
mv server/vestige-mcp.exe server/vestige-mcp-win32-x64.exe
mv server/vestige.exe server/vestige-win32-x64.exe
rm /tmp/win.zip

# Install the arch-dispatching launcher the manifest points at. The explicit
# destination path matters: check-manifest.sh resolves the manifest's command
# paths back to this script, so `cp launch.sh server/` (which hides the target
# name) would make that check unimplementable.
install -m 0755 launch.sh server/vestige-mcp-launch.sh

# Make executable
chmod +x server/*

# Fail here rather than after `mcpb pack`: every path the manifest points at
# must exist in server/, and every downloaded vestige-mcp-* binary must be
# reachable by the launcher or the manifest.
./check-manifest.sh --verify-server

echo "Binaries downloaded and verified. Run 'mcpb pack' to create bundle."
