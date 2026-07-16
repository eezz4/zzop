// Snapshot test: runs adapter.mjs as a subprocess against a minimal, inline-written fixture tree and
// deep-equals the parsed envelope JSON against a committed expected object. Fixture exercises the
// react-query-adapter's normalization edge cases (external https URL, `:param` colon segment,
// base-relative literal) — all delegated to adapter-kit's `resolveConsumeKey`/`normalizeProvideKey`
// (matching `zzop_core`/`parser-typescript`'s own consume-key semantics exactly), so this guards
// parity WITH the kit: an `https://` literal keys verbatim as an external consume, and a `:param`
// colon segment collapses to `{}` the same as a `{param}` template placeholder.
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
// their zero values when unset); this helper pads the sparse expectation to that shape.
function fileProjection({ path: p, loc, io }) {
  return {
    path: p,
    loc,
    symbols: [],
    imports: {},
    re_exports: [],
    dynamic_imports: [],
    used_names: [],
    const_map_fragment: {},
    procedure_router_fragments: [],
    router_mount_fragments: [],
    io,
    degraded: false,
    is_entry: false,
  };
}

function makeFixture() {
  const root = mkdtempSync(path.join(tmpdir(), 'react-query-adapter-test-'));
  mkdirSync(path.join(root, 'src'), { recursive: true });
  writeFileSync(
    path.join(root, 'src', 'articles.ts'),
    [
      "import { useQuery, useInfiniteQuery } from 'react-query';",
      '',
      'function useArticles(slug) {',
      '  return useQuery(`/articles/${slug}`);',
      '}',
      '',
      'function useTags() {',
      "  return useQuery('/tags');",
      '}',
      '',
      'function useFeed(filters) {',
      "  return useInfiniteQuery([`/articles${filters.feed ? '/feed' : ''}`, { limit: 10, ...filters }]);",
      '}',
      '',
      'function useExternal() {',
      "  return useQuery('https://api.example.com/v1/users');",
      '}',
      '',
      'function useColonParam() {',
      "  return useQuery('/articles/:slug/comments');",
      '}',
      '',
      'function useBaseRelative() {',
      "  return useQuery('users/login');",
      '}',
      '',
    ].join('\n')
  );
  return root;
}

test('react-query-adapter: envelope matches committed snapshot (incl. normalization edge cases)', () => {
  const root = makeFixture();
  try {
    const stdout = execFileSync(process.execPath, [ADAPTER, '--root', root, '--source', 'web'], {
      encoding: 'utf8',
    });
    const envelope = JSON.parse(stdout);
    assert.deepEqual(envelope, {
      format: 'zzop-normalized-ast',
      version: 1,
      parser: 'react-query-adapter/1',
      source: 'web',
      files: [
        fileProjection({
          path: 'src/articles.ts',
          loc: 26,
          io: {
            provides: [],
            consumes: [
              { kind: 'http', key: 'GET /articles/{}', file: 'src/articles.ts', line: 4 },
              { kind: 'http', key: 'GET /tags', file: 'src/articles.ts', line: 8 },
              { kind: 'http', key: 'GET /articles{}', file: 'src/articles.ts', line: 12 },
              // useExternal(): an absolute https:// literal keys VERBATIM as an external consume
              // (`resolveConsumeKey`'s `isExternalUrl` branch).
              { kind: 'http', key: 'GET https://api.example.com/v1/users', file: 'src/articles.ts', line: 16 },
              // useColonParam(): `:slug` collapses to `{}` (`normalizeProvideKey`'s `RE_PARAM`).
              { kind: 'http', key: 'GET /articles/{}/comments', file: 'src/articles.ts', line: 20 },
              { kind: 'http', key: 'GET /users/login', file: 'src/articles.ts', line: 24 },
            ],
          },
        }),
      ],
    });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
