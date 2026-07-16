#!/usr/bin/env node
// Mode B adapter: completes a Svelte / SvelteKit frontend's dep graph by projecting, per file, a
// FileProjection carrying dep-graph `imports` (giving TS modules imported only from `.svelte` files
// real fan-in) and/or `is_entry` (exempting SvelteKit framework entry files from `dead-candidates`),
// fed to the engine via `adapterOverlays`. Rationale and measured result: README.md.
//
// USAGE:  node adapter.mjs --root <webRoot> [--lib-alias '$lib=src/lib']  > overlay.json
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { walk, EnvelopeBuilder } from '../adapter-kit/index.js';

function arg(name, def) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : def;
}
const webRoot = arg('root');
if (!webRoot) {
  console.error("usage: node adapter.mjs --root <webRoot> [--lib-alias '$lib=src/lib']");
  process.exit(2);
}
const [aliasName, aliasTarget] = arg('lib-alias', '$lib=src/lib').split('=');

// SvelteKit framework entry conventions: loaded by filename, never imported.
const ENTRY_RE = /(^|\/)(hooks\.(client|server)|\+(page|layout|server|error)(\.(server|client))?)\.(ts|js|svelte)$/;

// Extract <script>...</script> bodies from a .svelte file (both default + context="module").
function svelteScript(text) {
  let body = '';
  const re = /<script\b[^>]*>([\s\S]*?)<\/script>/gi;
  let m;
  while ((m = re.exec(text))) body += m[1] + '\n';
  return body;
}

// Parse `import ... from '<spec>'` -> list of { local, spec }. Covers default, named, namespace.
function parseImports(code) {
  const out = [];
  const re = /import\s+(?:type\s+)?([^;'"]*?)\s+from\s*['"]([^'"]+)['"]/g;
  let m;
  while ((m = re.exec(code))) {
    const clause = m[1].trim();
    const spec = m[2];
    const locals = [];
    const named = clause.match(/\{([^}]*)\}/);
    if (named) {
      for (let s of named[1].split(',')) {
        s = s.trim().replace(/^type\s+/, '');
        if (!s) continue;
        locals.push(s.split(/\s+as\s+/).pop().trim());
      }
    }
    const head = clause.replace(/\{[^}]*\}/, '').replace(/,/g, ' ').trim();
    for (const tok of head.split(/\s+/)) {
      if (tok === '*' || tok === 'as') continue;
      if (/^[A-Za-z_$][\w$]*$/.test(tok)) locals.push(tok);
    }
    if (locals.length === 0) locals.push('_side'); // side-effect import still an edge
    for (const local of locals) out.push({ local, spec });
  }
  return out;
}

// Resolve a specifier to a path RELATIVE to the importing file's dir (so the engine resolves it with
// no alias config). Returns null for external/package specifiers (no internal edge).
function resolveSpecifier(spec, importerRel) {
  let targetRel;
  if (spec.startsWith('./') || spec.startsWith('../')) {
    targetRel = path.posix.normalize(path.posix.join(path.posix.dirname(importerRel), spec));
  } else if (aliasName && (spec === aliasName || spec.startsWith(aliasName + '/'))) {
    targetRel = spec.replace(aliasName, aliasTarget).replace(/^\/+/, '');
  } else {
    return null; // $app/*, bare packages, etc. — not an internal edge
  }
  let rel = path.posix.relative(path.posix.dirname(importerRel), targetRel);
  if (!rel.startsWith('.')) rel = './' + rel;
  return rel;
}

const builder = new EnvelopeBuilder({ parser: 'svelte-adapter/1', source: 'web' });
let fileCount = 0;
let edgeCount = 0;
let entryCount = 0;
for (const rel of walk(webRoot, { include: ['svelte', 'ts', 'js'], excludeFile: /\.(spec|test|d)\.[tj]s$/, skipDirs: ['.svelte-kit'] })) {
  const isSvelte = rel.endsWith('.svelte');
  const isEntry = ENTRY_RE.test('/' + rel);
  const text = readFileSync(path.join(webRoot, rel), 'utf8');
  const code = isSvelte ? svelteScript(text) : text;

  const imports = {};
  // Only project imports for .svelte (native TS files already have their imports parsed natively).
  if (isSvelte) {
    for (const { local, spec } of parseImports(code)) {
      const resolved = resolveSpecifier(spec, rel);
      if (!resolved) continue;
      const key = imports[local] ? `${local}$${Object.keys(imports).length}` : local;
      imports[key] = { specifier: resolved, original: local };
      edgeCount++;
    }
  }
  if (isEntry) entryCount++;

  if (Object.keys(imports).length > 0 || isEntry) {
    builder.addFile(rel, { loc: text.split('\n').length, imports });
    if (isEntry) builder.markEntry(rel);
    fileCount++;
  }
}

process.stderr.write(
  `[svelte-adapter] ${fileCount} projections, ${edgeCount} dep edges from .svelte, ${entryCount} SvelteKit entries\n`
);
process.stdout.write(JSON.stringify(builder.toEnvelope()));
