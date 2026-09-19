#!/bin/sh
# Vestige MCPB launcher — picks the binary that matches this machine.
#
# Why this file exists: the MCPB manifest can branch on the *platform* only.
# `mcp_config.platform_overrides` is keyed by `darwin` / `win32` / `linux`
# (MCPB manifest spec v0.3, "Platform-Specific Configurations"); there is no
# architecture key. A single hardcoded `command` was therefore correct for at
# most one binary per OS: Linux ARM64 and macOS Intel both got a binary built
# for another CPU and died in the dynamic loader with "exec format error"
# before the server ever logged a line.
#
# build.sh copies this script into `server/vestige-mcp-launch.sh` and the
# manifest points every non-Windows platform at it. Windows keeps an explicit
# `platform_overrides.win32` entry — there is no POSIX shell there to run this.
#
# `check-manifest.sh` fails the build when a bundled `vestige-mcp-*` binary is
# neither referenced by the manifest nor resolvable from the table below, so
# adding a target to build.sh without teaching this script about it is caught
# offline (CI: the `metadata` job).
set -eu

dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)
    exe="$dir/vestige-mcp-darwin-arm64"
    ;;
  Linux-x86_64)
    exe="$dir/vestige-mcp-linux-x64"
    ;;
  Linux-aarch64 | Linux-arm64)
    exe="$dir/vestige-mcp-linux-arm64"
    ;;
  *)
    echo "vestige-mcp: no bundled binary for $(uname -s)-$(uname -m)." >&2
    echo "Install for this platform instead: npm install -g vestige-mcp-server" >&2
    echo "(prebuilt archives: https://github.com/samvallad33/vestige/releases)" >&2
    exit 1
    ;;
esac

if [ ! -x "$exe" ]; then
  echo "vestige-mcp: bundled binary is missing or not executable: $exe" >&2
  exit 1
fi

exec "$exe" "$@"
