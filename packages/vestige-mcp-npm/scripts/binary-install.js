/**
 * Integrity + extraction helpers for the npm wrapper (`postinstall.js`).
 *
 * `release.yml` publishes each release archive together with a
 * `<archive>.sha256` sibling in `sha256sum` format (`<hex>  <filename>`).
 * `npm install vestige-mcp-server` downloads that archive and then *executes*
 * what is inside it, so the digest is the only thing standing between a
 * tampered (or truncated) download and code execution on the installer's
 * machine.
 *
 * Policy implemented here — fail closed:
 *
 * | checksum asset | behaviour                                             |
 * |----------------|-------------------------------------------------------|
 * | present, matches | extract                                            |
 * | present, differs | abort, nothing is extracted, archive deleted       |
 * | present, unparsable | abort (a mangled asset is not "no asset")       |
 * | 404 / 410 (older release) | extract + explicit "not verified" warning |
 * | other HTTP error / network failure | extract + warning              |
 *
 * Kept dependency-free (Node builtins only) so the test below can run in CI
 * without installing anything.
 */

'use strict';

const crypto = require('crypto');
const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');

const SHA256_HEX = /^[0-9a-f]{64}$/i;

/** Raised whenever an install must be aborted because integrity is unproven. */
class ChecksumError extends Error {
  constructor(message) {
    super(message);
    this.name = 'ChecksumError';
  }
}

/**
 * Extract the digest from a `sha256sum`-style file.
 *
 * Accepts the `<hex>  <filename>` form produced by `sha256sum`/`shasum -a 256`,
 * ignores blank lines and `#` comments, and returns the first 64-hex token.
 * Returns `null` when the text carries no usable digest.
 */
function parseChecksum(text) {
  if (typeof text !== 'string') return null;
  for (const line of text.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const token = trimmed.split(/\s+/)[0];
    if (SHA256_HEX.test(token)) return token.toLowerCase();
  }
  return null;
}

/** Streaming SHA-256 of a file, lower-case hex. */
function sha256File(filePath) {
  const hash = crypto.createHash('sha256');
  const fd = fs.openSync(filePath, 'r');
  try {
    const buffer = Buffer.allocUnsafe(1 << 20);
    let bytes;
    // eslint-disable-next-line no-cond-assign
    while ((bytes = fs.readSync(fd, buffer, 0, buffer.length, null)) > 0) {
      hash.update(buffer.subarray(0, bytes));
    }
  } finally {
    fs.closeSync(fd);
  }
  return hash.digest('hex');
}

/**
 * Constant-time digest comparison.
 *
 * A length mismatch is answered with a fast, safe `false` — `timingSafeEqual`
 * throws on differing lengths, and leaking "the length is wrong" is harmless
 * (a SHA-256 hex digest is always 64 characters or it is not a digest).
 */
function checksumsMatch(expected, actual) {
  if (typeof expected !== 'string' || typeof actual !== 'string') return false;
  const a = Buffer.from(expected.trim().toLowerCase(), 'utf8');
  const b = Buffer.from(actual.trim().toLowerCase(), 'utf8');
  if (a.length !== b.length) return false;
  return crypto.timingSafeEqual(a, b);
}

/**
 * Verify `archivePath` against the text of its `.sha256` asset.
 *
 * @returns {{expected: string, actual: string}}
 * @throws {ChecksumError} when the text is unusable or the digests differ.
 */
function verifyArchive(archivePath, checksumText) {
  const expected = parseChecksum(checksumText);
  if (!expected) {
    throw new ChecksumError(
      `The published checksum for ${path.basename(archivePath)} could not be parsed, ` +
        'so the download cannot be verified. Refusing to install it.',
    );
  }

  const actual = sha256File(archivePath);
  if (!checksumsMatch(expected, actual)) {
    throw new ChecksumError(
      `SHA-256 mismatch for ${path.basename(archivePath)}:\n` +
        `  published: ${expected}\n` +
        `  downloaded: ${actual}\n` +
        'The archive does not match the published release checksum, so it was NOT extracted.',
    );
  }

  return { expected, actual };
}

/** Classify the response to the `<archive>.sha256` request. */
function classifyChecksumResponse(statusCode, body) {
  if (statusCode === 200) return { kind: 'ok', text: body };
  if (statusCode === 404 || statusCode === 410) return { kind: 'missing' };
  return { kind: 'unavailable', reason: `HTTP ${statusCode}` };
}

/**
 * Unpack an archive. `run` is injectable so the tests never shell out to a
 * platform-specific extractor they do not control.
 */
function extract(archivePath, destDir, { isWindows = false, run = execFileSync } = {}) {
  if (isWindows) {
    run(
      'powershell',
      [
        '-NoProfile',
        '-Command',
        `Expand-Archive -Path '${archivePath}' -DestinationPath '${destDir}' -Force`,
      ],
      { stdio: 'inherit' },
    );
  } else {
    run('tar', ['-xzf', archivePath, '-C', destDir], { stdio: 'inherit' });
  }
}

/**
 * The install step of the wrapper: verify what can be verified, then extract.
 *
 * @param {object} args
 * @param {{kind: string, text?: string, reason?: string}} args.outcome  classification of the checksum fetch
 * @param {string} args.archivePath
 * @param {string} args.targetDir
 * @param {boolean} [args.isWindows]
 * @param {Function} [args.run]    injectable process runner
 * @param {Function} [args.warn]   warning sink (defaults to console.warn)
 * @returns {{verified: boolean, checksum?: string, reason?: string}}
 * @throws {ChecksumError} when a published checksum exists but does not match.
 */
function installWithChecksum({
  outcome,
  archivePath,
  targetDir,
  isWindows = false,
  run,
  warn = console.warn,
}) {
  if (outcome && outcome.kind === 'ok') {
    const { expected } = verifyArchive(archivePath, outcome.text);
    extract(archivePath, targetDir, { isWindows, run });
    return { verified: true, checksum: expected };
  }

  const reason = outcome && outcome.kind === 'missing' ? 'the release publishes no .sha256' : (outcome && outcome.reason) || 'the checksum could not be fetched';
  warn(
    `WARNING: integrity of ${path.basename(archivePath)} was NOT verified (${reason}).\n` +
      '         The binaries are about to be extracted and later executed without a digest check.\n' +
      '         Verify manually against https://github.com/samvallad33/vestige/releases if this matters to you.',
  );
  extract(archivePath, targetDir, { isWindows, run });
  return { verified: false, reason };
}

module.exports = {
  ChecksumError,
  checksumsMatch,
  classifyChecksumResponse,
  extract,
  installWithChecksum,
  parseChecksum,
  sha256File,
  verifyArchive,
};
