'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const { parseArgs, USAGE } = require('../bin/zzop.js');
const { buildSubcommandHelp } = require('../lib/help');

const BIN = path.join(__dirname, '..', 'bin', 'zzop.js');

/** Run the real CLI (`node bin/zzop.js <args>`) and return { status, stdout, stderr }. */
function runCli(args) {
  const res = spawnSync(process.execPath, [BIN, ...args], { encoding: 'utf8' });
  return { status: res.status, stdout: res.stdout, stderr: res.stderr };
}

// --- End-to-end: each subcommand's --help prints a focused block and exits 0 -----------------

test('zzop --help prints the full usage and exits 0', () => {
  const { status, stdout } = runCli(['--help']);
  assert.equal(status, 0);
  assert.equal(stdout, USAGE);
});

test('zzop init --help prints both init forms, exits 0', () => {
  for (const flag of ['--help', '-h']) {
    const { status, stdout } = runCli(['init', flag]);
    assert.equal(status, 0);
    assert.match(stdout, /zzop init \[--force\]/);
    assert.match(stdout, /zzop init adapter --mode/);
    // Focused: run options and the run exit-code table stay out of init's help.
    assert.doesNotMatch(stdout, /Options \(run\):/);
    assert.doesNotMatch(stdout, /Exit codes/);
  }
});

test('zzop init adapter --help prints the adapter scaffold form only, exits 0', () => {
  const { status, stdout } = runCli(['init', 'adapter', '--help']);
  assert.equal(status, 0);
  assert.match(stdout, /zzop init adapter --mode <a\|b> --kind <consume\|provide>/);
  assert.match(stdout, /--mode a = full envelope/);
  assert.doesNotMatch(stdout, /zzop init \[--force\]/);
  assert.doesNotMatch(stdout, /Options \(run\):/);
});

test('zzop run --help prints the run form with its options and exit codes, exits 0', () => {
  const { status, stdout } = runCli(['run', '--help']);
  assert.equal(status, 0);
  assert.match(stdout, /zzop \[run\] \[options\]/);
  assert.match(stdout, /Options \(run\):/);
  assert.match(stdout, /--severity <critical\|warning\|info\|off>/);
  assert.match(stdout, /Exit codes \(zzop \[run\]\):/);
  assert.doesNotMatch(stdout, /zzop init adapter/);
});

test('zzop adapter --help / adapter validate --help print the validate form, exit 0', () => {
  for (const args of [['adapter', '--help'], ['adapter', 'validate', '--help']]) {
    const { status, stdout } = runCli(args);
    assert.equal(status, 0);
    assert.match(stdout, /zzop adapter validate <path>/);
    assert.match(stdout, /`zzop adapter validate` ignores failOn/);
    assert.doesNotMatch(stdout, /Options \(run\):/);
  }
});

test('zzop pack --help / pack validate --help print the pack validate form, exit 0', () => {
  for (const args of [['pack', '--help'], ['pack', 'validate', '--help']]) {
    const { status, stdout } = runCli(args);
    assert.equal(status, 0);
    assert.match(stdout, /zzop pack validate <path>/);
    assert.match(stdout, /`zzop pack validate` ignores failOn/);
    // Focused: the sibling validator's entry stays out of pack's help, and vice-versa concerns.
    assert.doesNotMatch(stdout, /zzop adapter validate <path>/);
    assert.doesNotMatch(stdout, /Options \(run\):/);
  }
});

// --- Unit: slice anchors + command detection ---------------------------------------------------

test('every focused help block is a pure slice of USAGE (no re-typed lines)', () => {
  const usageLines = new Set(USAGE.split('\n'));
  const cases = [
    { command: 'init', commandGiven: true, initSubcommand: null },
    { command: 'init', commandGiven: true, initSubcommand: 'adapter' },
    { command: 'run', commandGiven: true, initSubcommand: null },
    { command: 'endpoint', commandGiven: true, initSubcommand: null },
    { command: 'adapter', commandGiven: true, initSubcommand: null },
    { command: 'pack', commandGiven: true, initSubcommand: null },
  ];
  for (const opts of cases) {
    const help = buildSubcommandHelp(USAGE, opts);
    assert.ok(help, `expected focused help for ${opts.command}`);
    for (const line of help.trimEnd().split('\n')) {
      // 'Usage:' and '' are themselves USAGE lines, so slice-purity covers the connective tissue too.
      assert.ok(usageLines.has(line), `help line not sliced from USAGE: "${line}"`);
    }
  }
});

test('bare --help and an unknown command fall back to the full USAGE (null)', () => {
  assert.equal(buildSubcommandHelp(USAGE, parseArgs(['--help'])), null);
  assert.equal(buildSubcommandHelp(USAGE, parseArgs(['bogus', '--help'])), null);
});

test('parseArgs marks whether a command was named (commandGiven)', () => {
  assert.equal(parseArgs([]).commandGiven, false);
  assert.equal(parseArgs(['--help']).commandGiven, false);
  assert.equal(parseArgs(['run']).commandGiven, true);
  assert.equal(parseArgs(['init', '--help']).commandGiven, true);
});
