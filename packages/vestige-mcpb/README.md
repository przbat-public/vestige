# Vestige MCPB

One-click installation bundle for Claude Desktop.

## For Users

1. Download `vestige-<version>.mcpb` (the version matches `manifest.json`) from
   [GitHub Releases](https://github.com/samvallad33/vestige/releases)
2. Double-click to install
3. Restart Claude Desktop

That's it. No npm, no terminal, no config files.

**Platform support.** The bundle contains one binary per OS+architecture:
macOS ARM64, Linux x86_64, Linux ARM64 and Windows x86_64. The three POSIX
targets are picked at launch by `launch.sh`, which reads `uname`; Windows is
picked earlier, by the manifest's `platform_overrides.win32`, because there is
no POSIX shell there to run the launcher. macOS Intel is not bundled — install
`npm install -g vestige-mcp-server` there instead; the launcher prints that
instruction rather than failing with `exec format error`.

## For Developers

### Building the bundle

```bash
# Install mcpb CLI
npm install -g @anthropic-ai/mcpb

# Download binaries from the GitHub release named in manifest.json
# (pass a tag to override: ./build.sh 3.4.0)
./build.sh

# Pack
mcpb pack
```

`build.sh` ends by running `./check-manifest.sh --verify-server`, which fails if
the manifest points at a file the build did not produce, or if a downloaded
`vestige-mcp-*` binary is unreachable from the manifest. The same script runs
without `--verify-server` in CI (`metadata` job), so a manifest/file mismatch is
caught on the PR instead of at pack time.

### Structure

```
vestige-mcpb/
├── manifest.json        # Bundle metadata (mcp_config → server/vestige-mcp-launch.sh)
├── launch.sh            # Dispatches to the binary matching this OS+arch
├── build.sh             # Downloads the release archives, installs launch.sh
├── check-manifest.sh    # manifest.json ↔ launch.sh ↔ build.sh consistency gate
├── server/              # Platform binaries (downloaded, gitignored)
│   ├── vestige-mcp-launch.sh
│   ├── vestige-mcp-darwin-arm64
│   ├── vestige-mcp-linux-x64
│   ├── vestige-mcp-linux-arm64
│   └── vestige-mcp-win32-x64.exe
└── vestige-<version>.mcpb   # Final bundle (generated, gitignored)
```

The MCPB manifest can only branch on the platform (`darwin` / `win32` / `linux`
in `mcp_config.platform_overrides`) — it has no architecture key — so Windows
uses an explicit override while every POSIX host goes through `launch.sh`.
