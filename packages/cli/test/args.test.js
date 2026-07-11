'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { parseArgs } = require('../bin/zzop.js');
const { ConfigError } = require('../lib/mapper');

test('defaults to the run command', () => {
  assert.equal(parseArgs([]).command, 'run');
});

test('run-only flags parse under run (explicit or default command)', () => {
  assert.equal(parseArgs(['--all']).all, true);
  assert.equal(parseArgs(['run', '-a']).all, true);
  assert.equal(parseArgs(['--json']).format, 'json');
  assert.equal(parseArgs(['--config', 'z.jsonc']).config, 'z.jsonc');
  assert.equal(parseArgs(['--out', 'zzop-reports']).out, 'zzop-reports');
});

test('--out requires a dir argument, and is rejected on init', () => {
  assert.throws(() => parseArgs(['--out']), (e) => e instanceof ConfigError && /--out requires/.test(e.message));
  assert.throws(() => parseArgs(['init', '--out', 'x']), (e) => e instanceof ConfigError && /not valid for the `init`/.test(e.message));
});

test('--force parses under init', () => {
  const opts = parseArgs(['init', '--force']);
  assert.equal(opts.command, 'init');
  assert.equal(opts.force, true);
});

test('a run-only flag on init is rejected', () => {
  assert.throws(() => parseArgs(['init', '--all']), (e) => e instanceof ConfigError && /not valid for the `init`/.test(e.message));
  assert.throws(() => parseArgs(['init', '--json']), ConfigError);
  assert.throws(() => parseArgs(['init', '--config', 'x']), ConfigError);
});

test('--force on run (incl. default command) is rejected', () => {
  assert.throws(() => parseArgs(['run', '--force']), (e) => e instanceof ConfigError && /not valid for the `run`/.test(e.message));
  assert.throws(() => parseArgs(['--force']), ConfigError);
});

test('--help / --version bypass command/flag validation', () => {
  // Global escape hatches must still work even alongside an otherwise-mismatched flag.
  assert.equal(parseArgs(['init', '--all', '--help']).help, true);
  assert.equal(parseArgs(['--force', '--version']).version, true);
});

test('version is full-name only (--version); -v is no longer an alias', () => {
  assert.equal(parseArgs(['--version']).version, true);
  assert.throws(() => parseArgs(['-v']), (e) => e instanceof ConfigError && /Unknown option "-v"/.test(e.message));
});

test('unknown option still errors', () => {
  assert.throws(() => parseArgs(['--nope']), ConfigError);
});

test('--severity parses each valid value and defaults to null', () => {
  assert.equal(parseArgs([]).severity, null);
  assert.equal(parseArgs(['--severity', 'critical']).severity, 'critical');
  assert.equal(parseArgs(['--severity', 'warning']).severity, 'warning');
  assert.equal(parseArgs(['--severity', 'info']).severity, 'info');
  assert.equal(parseArgs(['--severity', 'off']).severity, 'off');
});

test('--severity requires a value and rejects unknown values', () => {
  assert.throws(
    () => parseArgs(['--severity']),
    (e) => e instanceof ConfigError && /--severity requires/.test(e.message)
  );
  assert.throws(
    () => parseArgs(['--severity', 'warn']),
    (e) => e instanceof ConfigError && /Invalid severity "warn"/.test(e.message)
  );
});

test('--severity is run-scoped and rejected on init', () => {
  assert.throws(
    () => parseArgs(['init', '--severity', 'warning']),
    (e) => e instanceof ConfigError && /not valid for the `init`/.test(e.message)
  );
});

test('adapter validate <path> parses the subcommand and path', () => {
  const opts = parseArgs(['adapter', 'validate', 'envelope.json']);
  assert.equal(opts.command, 'adapter');
  assert.equal(opts.subcommand, 'validate');
  assert.equal(opts.envelopePath, 'envelope.json');
});

test('adapter validate requires a path argument', () => {
  assert.throws(
    () => parseArgs(['adapter', 'validate']),
    (e) => e instanceof ConfigError && /requires a <envelope\.json> path argument/.test(e.message)
  );
});

test('adapter requires the "validate" subcommand', () => {
  assert.throws(
    () => parseArgs(['adapter']),
    (e) => e instanceof ConfigError && /Unknown "adapter" subcommand ""/.test(e.message)
  );
  assert.throws(
    () => parseArgs(['adapter', 'bogus', 'envelope.json']),
    (e) => e instanceof ConfigError && /Unknown "adapter" subcommand "bogus"/.test(e.message)
  );
});

test('adapter validate rejects a run-scoped flag instead of silently ignoring it', () => {
  assert.throws(
    () => parseArgs(['adapter', 'validate', 'envelope.json', '--json']),
    (e) => e instanceof ConfigError && /not valid for the `adapter` command/.test(e.message)
  );
});

test('adapter validate rejects a trailing extra argument', () => {
  assert.throws(
    () => parseArgs(['adapter', 'validate', 'envelope.json', 'extra']),
    (e) => e instanceof ConfigError && /Unexpected argument "extra"/.test(e.message)
  );
});

test('--help bypasses adapter subcommand/path validation', () => {
  const opts = parseArgs(['adapter', '--help']);
  assert.equal(opts.help, true);
  assert.equal(opts.command, 'adapter');
});

test('--debug-io parses under run (explicit or default command) and defaults to false', () => {
  assert.equal(parseArgs([]).debugIo, false);
  assert.equal(parseArgs(['--debug-io']).debugIo, true);
  assert.equal(parseArgs(['run', '--debug-io']).debugIo, true);
});

test('--debug-io is run-scoped and rejected on init', () => {
  assert.throws(
    () => parseArgs(['init', '--debug-io']),
    (e) => e instanceof ConfigError && /not valid for the `init`/.test(e.message)
  );
});
