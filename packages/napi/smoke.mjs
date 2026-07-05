// Smoke test for the built addon (`zpz-napi.node`) — NOT part of `cargo test` (needs Node; see src/lib.rs's
// module doc). Run after the MSVC addon build:
//
//   cargo +stable-x86_64-pc-windows-msvc build -p zpz-napi --release --features addon
//   (copy/rename the produced DLL to packages/napi/zpz-napi.node)
//   node packages/napi/smoke.mjs
//
// Builds a tiny fixture tree with an import cycle, calls `analyze()`, and asserts the JSON parses and
// contains a non-empty `circular` finding. Also exercises `version()` and a basic `analyzeTrees()` call.

import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import native from './index.js';

function makeFixtureTree() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'zpz-napi-smoke-'));
  fs.writeFileSync(
    path.join(dir, 'a.ts'),
    "import { b } from './b';\nexport function a() { return b(); }\n"
  );
  fs.writeFileSync(
    path.join(dir, 'b.ts'),
    "import { a } from './a';\nexport function b() { return a(); }\n"
  );
  return dir;
}

function main() {
  const v = native.version();
  assert.equal(typeof v, 'string');
  assert.ok(v.includes('zpz-parser-typescript='), `version() missing TS fingerprint: ${v}`);
  console.log(`version(): ${v}`);

  const dir = makeFixtureTree();
  try {
    const configJson = JSON.stringify({ root: dir, sourceId: 'smoke' });
    const outJson = native.analyze(configJson);
    const out = JSON.parse(outJson); // throws if not valid JSON
    assert.equal(out.fileCount, 2, `expected 2 files, got ${out.fileCount}`);
    assert.ok(Array.isArray(out.findings) && out.findings.length > 0, 'expected non-empty findings');
    const cycle = out.findings.find((f) => f.ruleId === 'circular');
    assert.ok(cycle, `expected a circular finding, got: ${JSON.stringify(out.findings)}`);
    console.log(`analyze(): ${out.findings.length} finding(s), including a "circular" hit on ${cycle.file}`);

    // The fixture tree is not a git repo, and index.js now defaults `git: {}` into the config
    // (zero-config = full analysis), so the engine attempts git collection, fails, and reports it
    // via a warning instead of silently staying empty.
    assert.ok(
      Array.isArray(out.warnings) && out.warnings.some((w) => typeof w === 'string' && w.includes('git collection skipped')),
      `expected a "git collection skipped" warning, got: ${JSON.stringify(out.warnings)}`
    );

    const treesJson = JSON.stringify({
      trees: [
        { root: dir, sourceId: 'smoke-a' },
        { root: dir, sourceId: 'smoke-b' },
      ],
    });
    const multiOutJson = native.analyzeTrees(treesJson);
    const multiOut = JSON.parse(multiOutJson);
    assert.equal(multiOut.trees.length, 2, 'expected 2 trees in analyzeTrees() output');
    assert.ok(multiOut.crossLayer, 'expected a crossLayer field in analyzeTrees() output');
    console.log(`analyzeTrees(): ${multiOut.trees.length} tree(s) joined`);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }

  // analyzeEnvelope(): the docs/NORMALIZED_AST.md external-parser protocol receiver — a tiny
  // hand-built envelope (one JSP-shaped file with an http provide), no filesystem tree involved.
  const envelopeJson = JSON.stringify({
    format: 'zpz-normalized-ast',
    version: 1,
    parser: 'jsp-lexical/1',
    source: 'legacy',
    files: [
      {
        path: 'legacy/UserController.jsp',
        loc: 40,
        io: {
          provides: [
            {
              kind: 'http',
              key: 'GET /legacy/user.jsp',
              file: 'legacy/UserController.jsp',
              line: 5,
              symbol: 'getUser',
            },
          ],
          consumes: [],
        },
      },
    ],
  });
  const envelopeConfigJson = JSON.stringify({ sourceId: 'legacy' });
  const envelopeOutJson = native.analyzeEnvelope(envelopeJson, envelopeConfigJson);
  const envelopeOut = JSON.parse(envelopeOutJson);
  assert.equal(envelopeOut.fileCount, 1, `expected 1 file, got ${envelopeOut.fileCount}`);
  const provides = envelopeOut.ir?.io?.provides ?? [];
  assert.equal(provides.length, 1, `expected 1 io provide, got ${JSON.stringify(provides)}`);
  assert.equal(provides[0].key, 'GET /legacy/user.jsp');
  console.log(`analyzeEnvelope(): fileCount=${envelopeOut.fileCount}, io provide=${provides[0].key}`);

  // packsDir MERGE semantics: an explicit packsDir must not silently replace the bundled default
  // packs — index.js prepends the bundled dir, so a caller-supplied custom pack and a shipped
  // bundled rule both fire in the same run (docs/modules/napi.md's "Defaults" section).
  const mergeDir = fs.mkdtempSync(path.join(os.tmpdir(), 'zpz-napi-smoke-merge-'));
  const customPacksDir = fs.mkdtempSync(path.join(os.tmpdir(), 'zpz-napi-smoke-custom-packs-'));
  try {
    // `typescript/no-explicit-any` (rules/dsl/typescript.json) is the bundled rule exercised here —
    // its matcher is just the bare word `any` in a `.ts`/`.tsx` file, the simplest reliably-firing
    // shipped rule to trigger from a one-line fixture.
    fs.writeFileSync(path.join(mergeDir, 'any-type.ts'), 'export const bad: any = 1;\n');
    fs.writeFileSync(
      path.join(customPacksDir, 'smoke-custom.json'),
      JSON.stringify({
        id: 'smoke-custom',
        framework: 'any',
        rules: [
          {
            id: 'marker',
            severity: 'warning',
            message: 'smoke marker',
            matcher: {
              type: 'line-scan',
              file_pattern: '\\.ts$',
              line_pattern: 'SMOKE_CUSTOM_MARKER',
            },
          },
        ],
      })
    );
    fs.appendFileSync(path.join(mergeDir, 'any-type.ts'), '// SMOKE_CUSTOM_MARKER\n');

    const mergeConfigJson = JSON.stringify({
      root: mergeDir,
      sourceId: 'smoke-merge',
      packsDir: customPacksDir, // explicit packsDir — must MERGE with, not replace, the bundled dir
    });
    const mergeOutJson = native.analyze(mergeConfigJson);
    const mergeOut = JSON.parse(mergeOutJson);
    const mergeFindings = mergeOut.findings ?? [];
    assert.ok(
      mergeFindings.some((f) => f.ruleId === 'typescript/no-explicit-any'),
      `expected the bundled typescript/no-explicit-any rule to still fire alongside an explicit packsDir, got: ${JSON.stringify(mergeFindings)}`
    );
    assert.ok(
      mergeFindings.some((f) => f.ruleId === 'smoke-custom/marker'),
      `expected the custom pack's rule to fire, got: ${JSON.stringify(mergeFindings)}`
    );
    console.log(
      `analyze() packsDir merge: bundled + custom pack both fired (${mergeFindings.length} finding(s) total)`
    );
  } finally {
    fs.rmSync(mergeDir, { recursive: true, force: true });
    fs.rmSync(customPacksDir, { recursive: true, force: true });
  }

  console.log('ok');
}

main();
