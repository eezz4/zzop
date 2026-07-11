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
//   node adapter.mjs --mode consume --root <feRoot> --spec <openapi.json> [--sdk <import-specifier>] [--member-calls]
//   node adapter.mjs --mode provide --spec <openapi.json> [--source <id>] [--file <rel>]
// Writes the overlay envelope JSON to stdout; a one-line summary to stderr.
//
// --member-calls (default OFF, preserves prior behavior exactly): also match a call site when the
// operation name appears in MEMBER position (`.name(`) — e.g. `api.articles.getArticles(...)`,
// `this.api.getArticles(...)`. This is what a generated CLASS-METHOD client looks like
// (swagger-typescript-api, some openapi-generator targets): call sites are never a named import of
// the operationId, so the default named-import scan sees nothing at all. The safety rationale is
// unchanged: a member name only resolves if it (or its lowerFirst transform, see below) is also a
// spec operationId — lexical matching stays safe because that gate is still in force.
//
// --sdk in member mode: the existing `--sdk` substring gate (skip files that don't mention the
// specifier, before doing any regex work) still applies if you pass `--sdk`, and it works for ANY
// substring — an npm specifier (`@immich/sdk`) or a local/relative one (`src/services`) equally,
// since the check has always been a plain `text.includes(...)`, not an import-statement parse. If
// `--member-calls` is on and `--sdk` is NOT passed, the gate is skipped entirely (every walked file
// is scanned) — member calls don't depend on finding an import of the operation name, so there is
// nothing correct to gate on by default. Passing `--sdk` in member mode is still recommended for
// precision/perf when you know the local specifier.
//
// LIMITATIONS (intentional — a real adapter can go further): named-import call sites only
// (`import { getUser } from '<sdk>'`) unless `--member-calls` is on; namespace imports
// (`import * as sdk`) and re-exports are not followed. `type`-only imports are excluded. Call
// detection is lexical (`name(` / `.name(`), good enough given the operationId gate. The spec file
// must be JSON — YAML specs need a one-time conversion first (see README).
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
  const rawPath = `${basePath}${p}`;
  for (const [m, op] of Object.entries(methods)) {
    if (op && op.operationId && HTTP.test(m)) opMap.set(op.operationId, normalizeProvideKey(m, rawPath));
  }
}

// --member-calls name resolution: mainstream class-based generators (verified against
// swagger-typescript-api on fe-vue's committed api.ts, 19/19 operations) name each method by
// lower-casing only the operationId's first character — `GetArticles` -> `getArticles`, `Login` ->
// `login`. We accept both the raw operationId and this lowerFirst transform as valid member names.
// A candidate name is NEVER guessed past that: if two distinct operationIds collide on the same
// candidate (raw or transformed), that name is marked ambiguous and every call site using it is
// skipped, counted in the stderr summary instead of silently picked.
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
  const builder = new EnvelopeBuilder({ parser: 'openapi-sdk-adapter', source });
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

// The `--sdk` substring gate is skipped only when `--member-calls` is on AND `--sdk` was not passed
// at all — see the USAGE comment above for the rationale.
const gateSpecifier = memberCalls && !sdkGiven ? null : sdkSpecifier;
// Member-call scan: any `.identifier(` in a line, looked up against memberNameMap. Requiring the
// literal `.` immediately before the identifier (and `(` immediately after, whitespace aside) is
// what makes this safe against substring false-positives like `regetArticles(` — there is no `.`
// directly before `getArticles` in that string, so it never matches.
const memberCallRe = /\.([A-Za-z_$][\w$]*)\s*\(/g;

const builder = new EnvelopeBuilder({ parser: 'openapi-sdk-adapter', source });
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
