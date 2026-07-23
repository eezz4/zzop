'use strict';

// Tests bin/zzop.js's resolution + passthrough behavior. (a)/(b) exercise the dev-fallback path
// against the repo-local native binary — build it first with:
//   cargo build -p zzop-cli-bin --release
// (c) is a pure unit test of the platform map, no binary required.

const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const fs = require('node:fs');
const { spawnSync } = require('node:child_process');

const BIN = path.join(__dirname, '..', 'bin', 'zzop.js');
const DEV_BINARY = path.join(
  __dirname,
  '..',
  '..',
  '..',
  'target',
  'release',
  process.platform === 'win32' ? 'zzop.exe' : 'zzop'
);

function runCli(args) {
  const res = spawnSync(process.execPath, [BIN, ...args], { encoding: 'utf8' });
  return { status: res.status, stdout: res.stdout, stderr: res.stderr };
}

test('dev fallback: `zzop version` runs the repo-local release binary and exits 0', { skip: !fs.existsSync(DEV_BINARY) && 'repo-local release binary not built — run `cargo build -p zzop-cli-bin --release`' }, () => {
  const { status, stdout } = runCli(['version']);
  assert.equal(status, 0);
  assert.match(stdout, /\S/); // prints something (the native binary's own version text)
});

test('nonzero exit from the native binary propagates through the shim unchanged', { skip: !fs.existsSync(DEV_BINARY) && 'repo-local release binary not built — run `cargo build -p zzop-cli-bin --release`' }, () => {
  const { status } = runCli(['definitely-not-a-subcommand']);
  assert.notEqual(status, 0);
});

test('argument+stdio passthrough: `zzop version` through the shim is byte-identical to the binary run directly', { skip: !fs.existsSync(DEV_BINARY) && 'repo-local release binary not built — run `cargo build -p zzop-cli-bin --release`' }, () => {
  const viaShim = runCli(['version']);
  const direct = spawnSync(DEV_BINARY, ['version'], { encoding: 'utf8' });
  assert.equal(viaShim.status, 0);
  assert.equal(direct.status, 0);
  // Byte-identical stdout is the passthrough proof: the shim adds/alters nothing on the way to the
  // child process's own stdout.
  assert.equal(viaShim.stdout, direct.stdout);
});

test('argument+stdio passthrough: an args-carrying invocation (`zzop contract`) is byte-identical through the shim', { skip: !fs.existsSync(DEV_BINARY) && 'repo-local release binary not built — run `cargo build -p zzop-cli-bin --release`' }, () => {
  const viaShim = runCli(['contract']);
  const direct = spawnSync(DEV_BINARY, ['contract'], { encoding: 'utf8' });
  assert.equal(viaShim.status, 0);
  assert.equal(direct.status, 0);
  // `contract` (no name) lists every embedded doc — a multi-line, argument-shaped reply, unlike
  // `version`'s single line. Byte-identical stdout here additionally proves the shim passes the
  // subcommand argument through unmodified (not just "runs the binary with no args").
  assert.equal(viaShim.stdout, direct.stdout);
});

test('signal-kill mapping: a null spawnSync status (child killed by a signal) maps to exit 1', () => {
  // Lexical pin, not a live kill: actually signaling the child and racing spawnSync's status
  // collection is flaky on Windows (signal semantics differ from POSIX — there is no reliable way
  // to force a `status === null` result cross-platform without OS-specific scaffolding). Same
  // precedent as the platform-map test above: parse the shim's own source and assert the mapping
  // is there, rather than trying to trigger it end-to-end.
  const source = fs.readFileSync(BIN, 'utf8');
  assert.match(
    source,
    /result\.status === null \? 1 : result\.status/,
    'bin/zzop.js must map a null (signal-killed) spawnSync status to exit code 1, not propagate null/undefined into process.exit'
  );
});

test('platform map covers exactly the 5 supported platform/arch pairs', () => {
  // Re-parse the shim's source rather than requiring it (requiring would execute the CLI's
  // top-level spawnSync/resolution logic, which is not import-safe by design — this shim is a
  // pure launcher script, not a library).
  const source = fs.readFileSync(BIN, 'utf8');
  const match = /const PLATFORM_PACKAGES = (\{[\s\S]*?\n\});/.exec(source);
  assert.ok(match, 'PLATFORM_PACKAGES map not found in bin/zzop.js');

  // eslint-disable-next-line no-new-func -- reading a literal object out of trusted repo source.
  const PLATFORM_PACKAGES = new Function(`return ${match[1]}`)();

  assert.deepEqual(Object.keys(PLATFORM_PACKAGES).sort(), [
    'darwin-arm64',
    'darwin-x64',
    'linux-arm64',
    'linux-x64',
    'win32-x64',
  ]);

  assert.equal(PLATFORM_PACKAGES['win32-x64'].pkg, '@zzop/cli-win32-x64-msvc');
  assert.equal(PLATFORM_PACKAGES['win32-x64'].bin, 'zzop.exe');
  assert.equal(PLATFORM_PACKAGES['darwin-x64'].pkg, '@zzop/cli-darwin-x64');
  assert.equal(PLATFORM_PACKAGES['darwin-x64'].bin, 'zzop');
  assert.equal(PLATFORM_PACKAGES['darwin-arm64'].pkg, '@zzop/cli-darwin-arm64');
  assert.equal(PLATFORM_PACKAGES['linux-x64'].pkg, '@zzop/cli-linux-x64-gnu');
  assert.equal(PLATFORM_PACKAGES['linux-arm64'].pkg, '@zzop/cli-linux-arm64-gnu');
});
