#!/usr/bin/env node
// Reference "Mode B" adapter for zzop: makes a Svelte / SvelteKit frontend's dep graph complete, so
// TypeScript modules used only from `.svelte` files (invisible to zzop's TS parser) and SvelteKit
// framework entry files stop false-positiving `dead-candidates`.
//
// WHY THIS EXISTS
// zzop parses `.ts`/`.tsx` natively but not `.svelte`. A TS module imported ONLY by a `.svelte`
// component (e.g. a Svelte action `use:clickOutside`) therefore has fan-in 0 in the TS dep graph and
// looks dead. And SvelteKit convention files (`hooks.*`, `+page`/`+layout`/`+server`/`+error`) are
// loaded by the framework by filename, never imported, so their fan-in 0 is expected. Both are
// framework-specific, so — per zzop's direction — they are resolved by an INJECTED adapter, not by
// teaching the engine Svelte vocabulary. This adapter projects, per file, a FileProjection carrying
// dep-graph `imports` (giving the imported TS targets fan-in) and/or `is_entry` (exempting entries),
// fed to the engine via `adapterOverlays` (Mode B).
//
// USAGE:  node adapter.mjs --root <webRoot> [--lib-alias '$lib=src/lib']  > overlay.json
import { readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';

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

function walk(dir, out = []) {
  for (const e of readdirSync(dir)) {
    const abs = path.join(dir, e);
    const st = statSync(abs);
    if (st.isDirectory()) {
      if (e === 'node_modules' || e === '.git' || e === '.svelte-kit') continue;
      walk(abs, out);
    } else if (/\.(svelte|ts|js)$/.test(e) && !/\.(spec|test|d)\.[tj]s$/.test(e)) {
      out.push(abs);
    }
  }
  return out;
}

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

const files = [];
let edgeCount = 0;
let entryCount = 0;
for (const abs of walk(webRoot)) {
  const rel = path.relative(webRoot, abs).replace(/\\/g, '/');
  const isSvelte = rel.endsWith('.svelte');
  const isEntry = ENTRY_RE.test('/' + rel);
  const text = readFileSync(abs, 'utf8');
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
    files.push({
      path: rel,
      loc: text.split('\n').length,
      imports,
      is_entry: isEntry,
    });
  }
}

process.stderr.write(
  `[svelte-adapter] ${files.length} projections, ${edgeCount} dep edges from .svelte, ${entryCount} SvelteKit entries\n`
);
process.stdout.write(
  JSON.stringify({ format: 'zzop-normalized-ast', version: 1, parser: 'svelte-adapter', source: 'web', files })
);
