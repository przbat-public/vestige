#!/usr/bin/env node

const https = require('https');
const fs = require('fs');
const path = require('path');
const os = require('os');

const {
  ChecksumError,
  classifyChecksumResponse,
  installWithChecksum,
} = require('./binary-install');

const VERSION = require('../package.json').version;
// Release tag whose archives this wrapper installs. Derived from package.json, exactly
// like the published vestige-mcp-server@3.0.0: a hardcoded tag silently installs stale
// binaries the moment the wrapper is bumped (this file had drifted to 2.0.3).
const BINARY_VERSION = VERSION;
const PLATFORM = os.platform();
const ARCH = os.arch();

const PLATFORM_MAP = {
  darwin: 'apple-darwin',
  linux: 'unknown-linux-gnu',
  win32: 'pc-windows-msvc',
};

const ARCH_MAP = {
  x64: 'x86_64',
  arm64: 'aarch64',
};

const REFERENCE_URL = 'https://github.com/samvallad33/vestige/releases';

/** Truthy env switch (`1`, `true`, `yes`, `on`). */
function envTruthy(name) {
  const value = process.env[name];
  if (!value) return false;
  return ['1', 'true', 'yes', 'on'].includes(value.trim().toLowerCase());
}

const targetDir = path.join(__dirname, '..', 'bin');

// Escape hatch for offline installs, vendored binaries and CI images that bake
// the binaries in: skip the download entirely (and therefore the integrity
// check that guards it).
if (envTruthy('VESTIGE_SKIP_BINARY_DOWNLOAD')) {
  console.log('VESTIGE_SKIP_BINARY_DOWNLOAD is set — skipping the binary download.');
  console.log(`Expected location: ${targetDir}`);
  process.exit(0);
}

const platformStr = PLATFORM_MAP[PLATFORM];
const archStr = ARCH_MAP[ARCH];

if (!platformStr || !archStr) {
  console.error(`Unsupported platform: ${PLATFORM}-${ARCH}`);
  console.error('Supported: darwin/linux/win32 on x64/arm64');
  process.exit(1);
}

const target = `${archStr}-${platformStr}`;
const isWindows = PLATFORM === 'win32';
const archiveExt = isWindows ? 'zip' : 'tar.gz';
const archiveName = `vestige-mcp-${target}.${archiveExt}`;
const downloadUrl = `${REFERENCE_URL}/download/v${BINARY_VERSION}/${archiveName}`;
const checksumUrl = `${downloadUrl}.sha256`;

const archivePath = path.join(targetDir, archiveName);

console.log(`Installing Vestige MCP v${VERSION} for ${target}...`);

// Ensure bin directory exists
if (!fs.existsSync(targetDir)) {
  fs.mkdirSync(targetDir, { recursive: true });
}

/**
 * GET a URL, following redirects (GitHub releases redirect to a CDN), and
 * resolve with the decoded body.
 */
function fetchText(url, redirectsLeft = 5) {
  return new Promise((resolve, reject) => {
    https
      .get(url, (response) => {
        if (response.statusCode === 301 || response.statusCode === 302) {
          const redirectUrl = response.headers.location;
          response.resume();
          if (!redirectUrl || redirectsLeft === 0) {
            reject(new Error('Redirect loop or missing location header'));
            return;
          }
          resolve(fetchText(redirectUrl, redirectsLeft - 1));
          return;
        }

        const chunks = [];
        response.on('data', (chunk) => chunks.push(chunk));
        response.on('end', () =>
          resolve({ statusCode: response.statusCode, body: Buffer.concat(chunks).toString('utf8') }),
        );
      })
      .on('error', reject);
  });
}

/**
 * Resolve the checksum outcome without ever failing the install by itself:
 * "no checksum published" and "checksum unreachable" are reported as such and
 * handled by `installWithChecksum` (warning + proceed). A *mismatch*, on the
 * other hand, aborts the install — that decision lives in `binary-install.js`.
 */
async function fetchChecksumOutcome(url) {
  try {
    const { statusCode, body } = await fetchText(url);
    return classifyChecksumResponse(statusCode, body);
  } catch (err) {
    return { kind: 'unavailable', reason: err.message };
  }
}

/**
 * Download a file following redirects (GitHub releases use redirects)
 */
function download(url, dest) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest);

    const request = (currentUrl) => {
      https.get(currentUrl, (response) => {
        // Handle redirects (GitHub uses 302)
        if (response.statusCode === 301 || response.statusCode === 302) {
          const redirectUrl = response.headers.location;
          if (!redirectUrl) {
            reject(new Error('Redirect without location header'));
            return;
          }
          request(redirectUrl);
          return;
        }

        if (response.statusCode !== 200) {
          reject(new Error(`Download failed: HTTP ${response.statusCode}`));
          return;
        }

        response.pipe(file);
        file.on('finish', () => {
          file.close();
          resolve();
        });
      }).on('error', (err) => {
        fs.unlink(dest, () => {}); // Delete partial file
        reject(err);
      });
    };

    request(url);
  });
}

/**
 * Make binaries executable (Unix only)
 */
function makeExecutable(binDir) {
  if (isWindows) return;

  const binaries = ['vestige-mcp', 'vestige', 'vestige-restore'];
  for (const bin of binaries) {
    const binPath = path.join(binDir, bin);
    if (fs.existsSync(binPath)) {
      fs.chmodSync(binPath, 0o755);
    }
  }
}

async function main() {
  try {
    // Download
    console.log(`Downloading from ${downloadUrl}...`);
    await download(downloadUrl, archivePath);
    console.log('Download complete.');

    // Verify against the published `.sha256` before anything is unpacked.
    console.log('Verifying SHA-256...');
    const outcome = await fetchChecksumOutcome(checksumUrl);

    // Extract
    console.log('Extracting binaries...');
    const report = installWithChecksum({ outcome, archivePath, targetDir, isWindows });
    if (report.verified) {
      console.log(`Checksum verified (${report.checksum}).`);
    }

    // Cleanup archive
    fs.unlinkSync(archivePath);

    // Make executable
    makeExecutable(targetDir);

    // Verify installation
    const mcpBinary = path.join(targetDir, isWindows ? 'vestige-mcp.exe' : 'vestige-mcp');
    const cliBinary = path.join(targetDir, isWindows ? 'vestige.exe' : 'vestige');

    if (!fs.existsSync(mcpBinary)) {
      throw new Error('vestige-mcp binary not found after extraction');
    }

    console.log('');
    console.log('Vestige MCP installed successfully!');
    console.log('');
    console.log('Binaries installed:');
    console.log(`  - vestige-mcp: ${mcpBinary}`);
    if (fs.existsSync(cliBinary)) {
      console.log(`  - vestige:     ${cliBinary}`);
    }
    console.log('');
    console.log('Next steps:');
    console.log('  1. Add to Claude: claude mcp add vestige vestige-mcp -s user');
    console.log('  2. Restart Claude');
    console.log('  3. Test with: "remember that my favorite color is blue"');
    console.log('');

  } catch (err) {
    if (err instanceof ChecksumError) {
      // Fail closed: the archive was never handed to the extractor, and it is
      // removed so a corrupted download cannot be picked up later.
      fs.rmSync(archivePath, { force: true });
      console.error('');
      console.error('Integrity check FAILED — installation aborted.');
      console.error('');
      console.error(err.message);
      console.error('');
      console.error('Nothing was extracted. Please retry, and if it fails again report it:');
      console.error(`  ${REFERENCE_URL}/issues`);
      console.error('');
      process.exit(1);
    }

    console.error('');
    console.error('Installation failed:', err.message);
    console.error('');
    console.error('Manual installation:');
    console.error(`  1. Download: ${downloadUrl}`);
    console.error(`  2. Extract to: ${targetDir}`);
    console.error('  3. Ensure binaries are executable (chmod +x on Unix)');
    console.error('');
    process.exit(1);
  }
}

main();
