// Snapshot test: runs adapter.mjs as a subprocess against a minimal, inline-written fixture tree and
// deep-equals the parsed envelope JSON against a committed expected object. Fixture exercises: a `.ts`
// module imported only from a `.svelte` file (dep-graph fan-in), a `$lib` alias import, a SvelteKit
// `+page.ts` entry convention file, and a plain `.ts` file that should be projected as neither an
// import source nor an entry (and so must not appear in the output at all).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ADAPTER = path.join(__dirname, '..', 'adapter.mjs');

// adapter-kit's EnvelopeBuilder always emits the full FileProjection shape (all fields present, at
// their zero values when unset — including `io: {provides: [], consumes: []}`, which this adapter
// never populates); this helper pads the sparse expectation to that shape.
function fileProjection({ path: p, loc, imports, is_entry }) {
  return {
    path: p,
    loc,
    symbols: [],
    imports,
    re_exports: [],
    dynamic_imports: [],
    used_names: [],
    const_map_fragment: {},
    procedure_router_fragments: [],
    router_mount_fragments: [],
    io: { provides: [], consumes: [] },
    degraded: false,
    is_entry,
  };
}

function makeFixture() {
  const root = mkdtempSync(path.join(tmpdir(), 'svelte-adapter-test-'));
  mkdirSync(path.join(root, 'src', 'lib'), { recursive: true });
  mkdirSync(path.join(root, 'src', 'routes'), { recursive: true });
  writeFileSync(path.join(root, 'src', 'lib', 'util.ts'), 'export function clickOutside(node) { return node; }\n');
  writeFileSync(
    path.join(root, 'src', 'routes', 'App.svelte'),
    [
      '<script>',
      "  import { clickOutside } from '$lib/util';",
      "  import Foo from './Foo.svelte';",
      '</script>',
      '<div use:clickOutside></div>',
      '',
    ].join('\n')
  );
  writeFileSync(path.join(root, 'src', 'routes', 'Foo.svelte'), ['<script>', '</script>', '<div>foo</div>', ''].join('\n'));
  writeFileSync(path.join(root, 'src', 'routes', '+page.ts'), 'export const load = () => ({});\n');
  // A plain, un-imported-by-nothing .ts file — no imports of its own to project (native TS parsing
  // handles that natively) and not an entry convention file, so it must not appear in the output.
  writeFileSync(path.join(root, 'src', 'lib', 'unrelated.ts'), 'export const x = 1;\n');
  return root;
}

test('svelte-adapter: envelope matches committed snapshot', () => {
  const root = makeFixture();
  try {
    const stdout = execFileSync(process.execPath, [ADAPTER, '--root', root], { encoding: 'utf8' });
    const envelope = JSON.parse(stdout);
    assert.deepEqual(envelope, {
      format: 'zzop-normalized-ast',
      version: 1,
      parser: 'svelte-adapter/1',
      source: 'web',
      files: [
        fileProjection({ path: 'src/routes/+page.ts', loc: 2, imports: {}, is_entry: true }),
        fileProjection({
          path: 'src/routes/App.svelte',
          loc: 6,
          imports: {
            clickOutside: { specifier: '../lib/util', original: 'clickOutside' },
            Foo: { specifier: './Foo.svelte', original: 'Foo' },
          },
          is_entry: false,
        }),
      ],
    });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
