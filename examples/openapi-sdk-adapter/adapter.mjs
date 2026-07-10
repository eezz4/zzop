#!/usr/bin/env node
// Reference "Mode B" adapter for zzop: resolve a generated OpenAPI SDK client's call sites into
// cross-layer IO facts, emitted as a NormalizedEnvelope overlay for zzop's `adapterOverlays` config.
//
// WHY THIS EXISTS
// A frontend that talks to its backend only through a generated SDK client (e.g. `@immich/sdk`,
// `orval`, `openapi-typescript-codegen` output) never writes a literal `fetch('/x')` — it calls
// `getUser(id)`. zzop's native egress extractor cannot see the route, so the cross-layer join is
// blind for that source. This adapter fills the gap WITHOUT teaching the engine any SDK-specific
// vocabulary: it reads the committed OpenAPI spec (where `operationId` === the SDK's exported
// function name for every mainstream generator) and projects each FE call site as an `IoConsume`,
// and/or each spec operation as an `IoProvide`. The engine merges these on top of native analysis
// via the `adapterOverlays` field (see docs/NORMALIZED_AST.md).
//
// USAGE
//   node adapter.mjs --mode consume --root <feRoot> --spec <openapi.json> [--sdk <import-specifier>]
//   node adapter.mjs --mode provide --spec <openapi.json> [--source <id>] [--file <rel>]
// Writes the overlay envelope JSON to stdout; a one-line summary to stderr.
//
// LIMITATIONS (intentional — a real adapter can go further): named-import call sites only
// (`import { getUser } from '<sdk>'`); namespace imports (`import * as sdk`) and re-exports are not
// followed. `type`-only imports are excluded. Call detection is lexical (`name(`), good enough given
// the operationId gate.
import { readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';

function arg(name, def) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : def;
}

const mode = arg('mode', 'consume');
const specPath = arg('spec');
const feRoot = arg('root');
const sdkSpecifier = arg('sdk', '@immich/sdk');
const source = arg('source', 'api');
const provideFile = arg('file', 'openapi.spec');
if (!specPath || (mode === 'consume' && !feRoot)) {
  console.error('usage: node adapter.mjs --mode <consume|provide> --spec <openapi.json> [--root <feRoot>] [--sdk <specifier>]');
  process.exit(2);
}

// operationId -> "METHOD /path", with zzop's http_interface_key normalization ({param} -> {}).
//
// The served route is `servers[].url`'s PATH PART + the paths key (OpenAPI's effective-URL rule) —
// immich, for example, declares `servers: [{"url": "/api"}]` and serves `/api/activities`, while
// `paths` only says `/activities`. Skipping the base made every emitted key one prefix short of the
// backend tree's real provides: 0 exact joins, 349 route-near-miss "missing `/api`" findings on the
// immich pair. A server url with template variables (`{region}.host/v2`) contributes only what is
// static — if its path part itself is templated, we fall back to no prefix rather than guess.
const spec = JSON.parse(readFileSync(specPath, 'utf8'));
const serverUrl = (Array.isArray(spec.servers) && spec.servers[0] && spec.servers[0].url) || '';
let basePath = '';
if (serverUrl) {
  // Relative form ("/api") is already a path; absolute form contributes only its pathname.
  const rawPath = /^[a-z][a-z0-9+.-]*:\/\//i.test(serverUrl)
    ? serverUrl.replace(/^[a-z][a-z0-9+.-]*:\/\/[^/]*/i, '')
    : serverUrl;
  if (!rawPath.includes('{')) basePath = rawPath.replace(/\/+$/, '');
}
const opMap = new Map();
const HTTP = /^(get|post|put|patch|delete|head|options)$/i;
for (const [p, methods] of Object.entries(spec.paths || {})) {
  const norm =
    `${basePath}${p}`.replace(/\{[^}]+\}/g, '{}').replace(/\/+/g, '/').replace(/\/$/, '') || '/';
  for (const [m, op] of Object.entries(methods)) {
    if (op && op.operationId && HTTP.test(m)) opMap.set(op.operationId, `${m.toUpperCase()} ${norm}`);
  }
}

function envelope(files) {
  return { format: 'zzop-normalized-ast', version: 1, parser: 'openapi-sdk-adapter', source, files };
}

if (mode === 'provide') {
  // Every spec operation as an IoProvide — use this when you have the OpenAPI spec but not the
  // backend tree itself (otherwise zzop extracts provides natively from the BE framework).
  const provides = [...opMap.values()].map((key) => ({ kind: 'http', key, file: provideFile, line: 1 }));
  process.stderr.write(`[openapi-sdk-adapter] provide: ${provides.length} operations\n`);
  process.stdout.write(JSON.stringify(envelope([{ path: provideFile, loc: 1, io: { provides, consumes: [] } }])));
  process.exit(0);
}

// mode === 'consume': scan FE files for SDK-imported operationId call sites.
function walk(dir, out = []) {
  for (const e of readdirSync(dir)) {
    const abs = path.join(dir, e);
    const st = statSync(abs);
    if (st.isDirectory()) {
      if (e === 'node_modules' || e === '.git') continue;
      walk(abs, out);
    } else if (/\.(ts|tsx|js|jsx|mjs|svelte|vue)$/.test(e) && !/\.(spec|test)\.[tj]sx?$/.test(e)) {
      out.push(abs);
    }
  }
  return out;
}

// Value (non-type) local names imported from the SDK specifier.
function sdkValueImports(text, specifier) {
  const names = new Set();
  const esc = specifier.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const re = new RegExp(`import\\s*(?:type\\s+)?\\{([^}]*)\\}\\s*from\\s*['"]${esc}['"]`, 'g');
  let m;
  while ((m = re.exec(text))) {
    if (/import\s+type\s*\{/.test(m[0])) continue;
    for (let s of m[1].split(',')) {
      s = s.trim();
      if (!s || /^type\s/.test(s)) continue;
      const local = s.split(/\s+as\s+/).pop().trim();
      if (/^[A-Za-z_$][\w$]*$/.test(local)) names.add(local);
    }
  }
  return names;
}

const files = [];
let calls = 0;
const ops = new Set();
for (const abs of walk(feRoot)) {
  const text = readFileSync(abs, 'utf8');
  if (!text.includes(sdkSpecifier)) continue;
  const callable = [...sdkValueImports(text, sdkSpecifier)].filter((n) => opMap.has(n));
  if (!callable.length) continue;
  const rel = path.relative(feRoot, abs).replace(/\\/g, '/');
  const lines = text.split('\n');
  const consumes = [];
  for (let i = 0; i < lines.length; i++) {
    for (const name of callable) {
      if (new RegExp(`(^|[^.\\w$])${name}\\s*\\(`).test(lines[i])) {
        consumes.push({ kind: 'http', key: opMap.get(name), file: rel, line: i + 1 });
        calls++;
        ops.add(name);
      }
    }
  }
  if (consumes.length) files.push({ path: rel, loc: lines.length, io: { provides: [], consumes } });
}
process.stderr.write(
  `[openapi-sdk-adapter] consume: ${files.length} files, ${calls} call sites, ${ops.size}/${opMap.size} operations resolved\n`
);
process.stdout.write(JSON.stringify(envelope(files)));
