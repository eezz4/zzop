#!/usr/bin/env node
// Mode B adapter: resolve a generated OpenAPI SDK client's call sites into cross-layer IO facts,
// emitted as a NormalizedEnvelope overlay for zzop's `adapterOverlays` config (docs/NORMALIZED_AST.md).
// Keys come from the committed OpenAPI spec: `operationId` === the SDK's exported function name for
// every mainstream generator.
//
// USAGE
//   node adapter.mjs --mode consume --root <feRoot> --spec <openapi.json> [--sdk <import-specifier>] [--member-calls]
//   node adapter.mjs --mode provide --spec <openapi.json> [--source <id>] [--file <rel>]
// Overlay envelope JSON to stdout; one-line summary to stderr.
//
// CONTRACT CONSTRAINTS
// - Named-import mode (default): a call site counts only if the name is a VALUE import from the
//   `--sdk` specifier AND a spec operationId. `type`-only imports are excluded; namespace imports
//   (`import * as sdk`) and re-exports are not followed.
// - `--member-calls` (default OFF): also match `.name(` in member position (generated CLASS-METHOD
//   clients, e.g. `api.articles.getArticles(...)`). A member name resolves only if it, or its
//   lowerFirst transform, is a spec operationId — that gate is what keeps lexical matching safe.
// - `--sdk` gate: a plain `text.includes(...)` substring pre-filter (npm or local/relative specifier
//   alike), never an import-statement parse. Skipped only when `--member-calls` is on AND `--sdk`
//   was not passed — member calls don't depend on an import of the operation name, so there is
//   nothing correct to gate on by default; passing `--sdk` is still recommended for precision/perf.
// - Call detection is lexical (`name(` / `.name(`). The spec file must be JSON — YAML specs need a
//   one-time external conversion first (see README).
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { walk, normalizeProvideKey, EnvelopeBuilder } from '../adapter-kit/index.js';

function arg(name, def) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : def;
}

const mode = arg('mode', 'consume');
const specPath = arg('spec');
const feRoot = arg('root');
const sdkSpecifier = arg('sdk', '@immich/sdk');
const sdkGiven = process.argv.includes('--sdk');
const memberCalls = process.argv.includes('--member-calls');
const source = arg('source', 'api');
const provideFile = arg('file', 'openapi.spec');
if (!specPath || (mode === 'consume' && !feRoot)) {
  console.error('usage: node adapter.mjs --mode <consume|provide> --spec <openapi.json> [--root <feRoot>] [--sdk <specifier>]');
  process.exit(2);
}

// operationId -> "METHOD /path", with zzop's http_interface_key normalization ({param} -> {}).
// The served route is `servers[].url`'s PATH PART + the paths key (OpenAPI's effective-URL rule);
// skipping the base prefix leaves every emitted key one prefix short of the backend's real provides.
// A server url with template variables (`{region}.host/v2`) contributes only what is static — if its
// path part itself is templated, we fall back to no prefix rather than guess.
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
  const rawPath = `${basePath}${p}`;
  for (const [m, op] of Object.entries(methods)) {
    if (op && op.operationId && HTTP.test(m)) opMap.set(op.operationId, normalizeProvideKey(m, rawPath));
  }
}

// --member-calls name resolution: class-based generators name each method by lower-casing only the
// operationId's first character (`GetArticles` -> `getArticles`). Both the raw operationId and this
// lowerFirst transform are valid member names — NEVER anything further: if two distinct operationIds
// collide on the same candidate (raw or transformed), that name is marked ambiguous and every call
// site using it is skipped, counted in the stderr summary instead of silently picked.
const memberNameMap = new Map(); // member name -> operationId | 'AMBIGUOUS'
if (memberCalls) {
  for (const opId of opMap.keys()) {
    const candidates = new Set([opId, opId.charAt(0).toLowerCase() + opId.slice(1)]);
    for (const cand of candidates) {
      if (!memberNameMap.has(cand)) memberNameMap.set(cand, opId);
      else if (memberNameMap.get(cand) !== opId) memberNameMap.set(cand, 'AMBIGUOUS');
    }
  }
}
const ambiguousMemberNames = [...memberNameMap.entries()]
  .filter(([, v]) => v === 'AMBIGUOUS')
  .map(([k]) => k);

if (mode === 'provide') {
  // Every spec operation as an IoProvide — use this when you have the OpenAPI spec but not the
  // backend tree itself (otherwise zzop extracts provides natively from the BE framework).
  const builder = new EnvelopeBuilder({ parser: 'openapi-sdk-adapter/1', source });
  builder.addFile(provideFile, { loc: 1 });
  for (const key of opMap.values()) builder.addProvide(provideFile, { kind: 'http', key, line: 1 });
  process.stderr.write(`[openapi-sdk-adapter] provide: ${opMap.size} operations\n`);
  process.stdout.write(JSON.stringify(builder.toEnvelope()));
  process.exit(0);
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

// Gate skipped only when `--member-calls` is on AND `--sdk` was not passed — see header.
const gateSpecifier = memberCalls && !sdkGiven ? null : sdkSpecifier;
// Member-call scan: any `.identifier(` in a line, looked up against memberNameMap. Requiring the
// literal `.` immediately before the identifier (and `(` immediately after, whitespace aside) keeps
// this safe against substring false-positives like `regetArticles(`.
const memberCallRe = /\.([A-Za-z_$][\w$]*)\s*\(/g;

const builder = new EnvelopeBuilder({ parser: 'openapi-sdk-adapter/1', source });
let fileCount = 0;
let calls = 0;
const ops = new Set();
for (const rel of walk(feRoot, { include: ['ts', 'tsx', 'js', 'jsx', 'mjs', 'svelte', 'vue'], excludeFile: /\.(spec|test)\.[tj]sx?$/ })) {
  const text = readFileSync(path.join(feRoot, rel), 'utf8');
  if (gateSpecifier && !text.includes(gateSpecifier)) continue;
  const callable = [...sdkValueImports(text, sdkSpecifier)].filter((n) => opMap.has(n));
  if (!callable.length && !memberCalls) continue;
  const lines = text.split('\n');
  const consumes = [];
  for (let i = 0; i < lines.length; i++) {
    for (const name of callable) {
      if (new RegExp(`(^|[^.\\w$])${name}\\s*\\(`).test(lines[i])) {
        consumes.push({ key: opMap.get(name), line: i + 1 });
        calls++;
        ops.add(name);
      }
    }
    if (memberCalls) {
      memberCallRe.lastIndex = 0;
      const seenOnLine = new Set();
      let mm;
      while ((mm = memberCallRe.exec(lines[i]))) {
        const opId = memberNameMap.get(mm[1]);
        if (!opId || opId === 'AMBIGUOUS' || seenOnLine.has(opId)) continue;
        seenOnLine.add(opId);
        consumes.push({ key: opMap.get(opId), line: i + 1 });
        calls++;
        ops.add(opId);
      }
    }
  }
  if (consumes.length) {
    builder.addFile(rel, { loc: lines.length });
    for (const c of consumes) builder.addConsume(rel, { kind: 'http', key: c.key, line: c.line });
    fileCount++;
  }
}
const memberSummary = memberCalls
  ? `; member-calls: ${ambiguousMemberNames.length} ambiguous name(s) skipped` +
    (ambiguousMemberNames.length ? ` (${ambiguousMemberNames.join(', ')})` : '')
  : '';
process.stderr.write(
  `[openapi-sdk-adapter] consume: ${fileCount} files, ${calls} call sites, ${ops.size}/${opMap.size} operations resolved${memberSummary}\n`
);
process.stdout.write(JSON.stringify(builder.toEnvelope()));
