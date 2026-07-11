'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const { execFileSync } = require('node:child_process');

const { parseArgs } = require('../bin/zzop.js');
const { ConfigError } = require('../lib/mapper');
const { buildAdapterScaffold, ADAPTER_MODE_VALUES, ADAPTER_KIND_VALUES } = require('../lib/adapter-templates');

const ZZOP_BIN = path.join(__dirname, '..', 'bin', 'zzop.js');

function mkTmpDir() {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'zzop-init-adapter-'));
}

// --- routing / flag parsing -------------------------------------------------------------------------

test('parseArgs: "init adapter --mode <a|b> --kind <consume|provide>" parses cleanly', () => {
  for (const mode of ADAPTER_MODE_VALUES) {
    for (const kind of ADAPTER_KIND_VALUES) {
      const opts = parseArgs(['init', 'adapter', '--mode', mode, '--kind', kind]);
      assert.equal(opts.command, 'init');
      assert.equal(opts.initSubcommand, 'adapter');
      assert.equal(opts.mode, mode);
      assert.equal(opts.kind, kind);
    }
  }
});

test('parseArgs: "init adapter" without --mode/--kind is rejected with ConfigError', () => {
  assert.throws(
    () => parseArgs(['init', 'adapter']),
    (e) => e instanceof ConfigError && /requires --mode <a\|b> and --kind <consume\|provide>/.test(e.message)
  );
  assert.throws(
    () => parseArgs(['init', 'adapter', '--mode', 'a']),
    (e) => e instanceof ConfigError && /requires --kind <consume\|provide>/.test(e.message)
  );
  assert.throws(
    () => parseArgs(['init', 'adapter', '--kind', 'consume']),
    (e) => e instanceof ConfigError && /requires --mode <a\|b>/.test(e.message)
  );
});

test('parseArgs: invalid --mode/--kind values are rejected with ConfigError', () => {
  assert.throws(
    () => parseArgs(['init', 'adapter', '--mode', 'c', '--kind', 'consume']),
    (e) => e instanceof ConfigError && /Invalid mode "c"/.test(e.message)
  );
  assert.throws(
    () => parseArgs(['init', 'adapter', '--mode', 'a', '--kind', 'bogus']),
    (e) => e instanceof ConfigError && /Invalid kind "bogus"/.test(e.message)
  );
  assert.throws(
    () => parseArgs(['init', 'adapter', '--mode']),
    (e) => e instanceof ConfigError && /--mode requires <a\|b>/.test(e.message)
  );
  assert.throws(
    () => parseArgs(['init', 'adapter', '--mode', 'a', '--kind']),
    (e) => e instanceof ConfigError && /--kind requires <consume\|provide>/.test(e.message)
  );
});

test('parseArgs: --mode/--kind are rejected on bare "init" (not scoped to "init adapter")', () => {
  assert.throws(
    () => parseArgs(['init', '--mode', 'a', '--kind', 'consume']),
    (e) => e instanceof ConfigError && /"--mode" is only valid with `zzop init adapter`/.test(e.message)
  );
});

test('parseArgs: --mode/--kind are rejected on "run"/"adapter validate"', () => {
  assert.throws(
    () => parseArgs(['--mode', 'a']),
    (e) => e instanceof ConfigError && /"--mode" is only valid with `zzop init adapter`/.test(e.message)
  );
  assert.throws(
    () => parseArgs(['adapter', 'validate', 'x.json', '--kind', 'consume']),
    (e) => e instanceof ConfigError && /"--kind" is only valid with `zzop init adapter`/.test(e.message)
  );
});

test('parseArgs: unknown "init" subcommand is rejected', () => {
  assert.throws(
    () => parseArgs(['init', 'bogus']),
    (e) => e instanceof ConfigError && /Unknown "init" subcommand "bogus"/.test(e.message)
  );
});

test('parseArgs: --force still parses under "init adapter"', () => {
  const opts = parseArgs(['init', 'adapter', '--mode', 'a', '--kind', 'consume', '--force']);
  assert.equal(opts.force, true);
});

test('parseArgs: a trailing extra positional after "init adapter" is rejected', () => {
  assert.throws(
    () => parseArgs(['init', 'adapter', 'extra']),
    (e) => e instanceof ConfigError && /Unexpected argument "extra"/.test(e.message)
  );
});

test('parseArgs: bare "init" is unaffected (no initSubcommand, mode/kind stay null)', () => {
  const opts = parseArgs(['init']);
  assert.equal(opts.command, 'init');
  // `rest[1]` is `undefined` (not the field's `null` default) when no second positional was given —
  // matches how `adapter`'s own `opts.subcommand` behaves for the same reason.
  assert.equal(opts.initSubcommand, undefined);
  assert.equal(opts.mode, null);
  assert.equal(opts.kind, null);
});

// --- scaffold content (buildAdapterScaffold) --------------------------------------------------------

test('buildAdapterScaffold: file-set snapshot is stable across mode/kind combos', () => {
  for (const mode of ADAPTER_MODE_VALUES) {
    for (const kind of ADAPTER_KIND_VALUES) {
      const files = buildAdapterScaffold({ mode, kind });
      assert.deepEqual(
        files.map((f) => f.name).sort(),
        ['README.md', 'lib/envelope.mjs', 'lib/keys.mjs', 'main.mjs'].sort()
      );
      for (const f of files) {
        assert.equal(typeof f.content, 'string');
        assert.ok(f.content.length > 0, `${f.name} (mode ${mode}, kind ${kind}) is empty`);
      }
    }
  }
});

test('buildAdapterScaffold: throws on an invalid mode/kind', () => {
  assert.throws(() => buildAdapterScaffold({ mode: 'c', kind: 'consume' }), /invalid mode/);
  assert.throws(() => buildAdapterScaffold({ mode: 'a', kind: 'bogus' }), /invalid kind/);
});

test('buildAdapterScaffold: bundled lib/keys.mjs and lib/envelope.mjs carry a source-of-truth header comment', () => {
  const files = buildAdapterScaffold({ mode: 'a', kind: 'consume' });
  const keys = files.find((f) => f.name === 'lib/keys.mjs').content;
  const envelope = files.find((f) => f.name === 'lib/envelope.mjs').content;

  assert.match(keys, /SOURCE OF TRUTH: examples\/adapter-kit\/lib\/keys\.js/);
  assert.match(keys, /docs\/adapters\/key-normalization\.fixture\.json/);
  assert.match(keys, /replay/i);

  assert.match(envelope, /SOURCE OF TRUTH: examples\/adapter-kit\/lib\/envelope\.js/);
});

test('buildAdapterScaffold: main.mjs carries TODO(vocabulary) markers and the requested mode/kind wiring', () => {
  const consumeMain = buildAdapterScaffold({ mode: 'a', kind: 'consume' }).find((f) => f.name === 'main.mjs')
    .content;
  assert.match(consumeMain, /TODO\(vocabulary\)/);
  assert.match(consumeMain, /resolveConsumeKey/);
  assert.doesNotMatch(consumeMain, /normalizeProvideKey/);

  const provideMain = buildAdapterScaffold({ mode: 'b', kind: 'provide' }).find((f) => f.name === 'main.mjs')
    .content;
  assert.match(provideMain, /normalizeProvideKey/);
  assert.doesNotMatch(provideMain, /resolveConsumeKey/);
  // Mode B is the io-only overlay — its main.mjs should point at the `overlays` config key.
  assert.match(provideMain, /overlays/);
});

test('buildAdapterScaffold: generated main.mjs/lib files are syntactically valid ESM and runnable', () => {
  const dir = mkTmpDir();
  const files = buildAdapterScaffold({ mode: 'a', kind: 'consume' });
  for (const f of files) {
    const dest = path.join(dir, f.name);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.writeFileSync(dest, f.content, 'utf8');
  }
  // No throw == valid syntax; also actually run it end to end against an empty-ish root (itself) to
  // confirm the envelope it prints parses and satisfies the v1 contract shape.
  const stdout = execFileSync(process.execPath, [path.join(dir, 'main.mjs'), dir], { encoding: 'utf8' });
  const envelope = JSON.parse(stdout);
  assert.equal(envelope.format, 'zzop-normalized-ast');
  assert.equal(envelope.version, 1);
  assert.ok(Array.isArray(envelope.files));
});

// --- CLI wiring (`zzop init adapter`, run as a real subprocess) --------------------------------------

test('zzop init adapter: writes the scaffold into ./zzop-adapter/ and refuses to overwrite without --force', () => {
  const dir = mkTmpDir();
  const r1 = execFileSync(process.execPath, [ZZOP_BIN, 'init', 'adapter', '--mode', 'a', '--kind', 'consume'], {
    cwd: dir,
    encoding: 'utf8',
  });
  assert.match(r1, /Wrote 4 files to zzop-adapter/);
  assert.ok(fs.existsSync(path.join(dir, 'zzop-adapter', 'main.mjs')));
  assert.ok(fs.existsSync(path.join(dir, 'zzop-adapter', 'lib', 'keys.mjs')));
  assert.ok(fs.existsSync(path.join(dir, 'zzop-adapter', 'lib', 'envelope.mjs')));
  assert.ok(fs.existsSync(path.join(dir, 'zzop-adapter', 'README.md')));

  // Re-running without --force refuses (exit 2), mirroring `zzop init`'s existing-config refusal.
  assert.throws(() => {
    execFileSync(process.execPath, [ZZOP_BIN, 'init', 'adapter', '--mode', 'b', '--kind', 'provide'], {
      cwd: dir,
      encoding: 'utf8',
      stdio: 'pipe',
    });
  }, (err) => {
    assert.equal(err.status, 2);
    assert.match(err.stderr, /zzop-adapter\/ already exists\. Use --force to overwrite\./);
    return true;
  });

  // --force overwrites, and the mode/kind switch is reflected in the regenerated main.mjs.
  execFileSync(
    process.execPath,
    [ZZOP_BIN, 'init', 'adapter', '--mode', 'b', '--kind', 'provide', '--force'],
    { cwd: dir, encoding: 'utf8' }
  );
  const main = fs.readFileSync(path.join(dir, 'zzop-adapter', 'main.mjs'), 'utf8');
  assert.match(main, /normalizeProvideKey/);
});
