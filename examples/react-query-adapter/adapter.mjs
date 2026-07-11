#!/usr/bin/env node
// Reference "Mode B" adapter for zzop: resolve react-query v3's positional-key idiom into cross-layer
// IO facts, emitted as a NormalizedEnvelope overlay for zzop's `adapterOverlays` config.
//
// WHY THIS EXISTS
// react-query v3's common wiring is ONE default `queryFn` registered on the client
// (`axios.get(queryKey[0], { params: queryKey[1] })`), and every read call is just:
//
//     useQuery('/tags')
//     useQuery(`/articles/${slug}`)
//     useQuery([`/articles${filters.feed ? '/feed' : ''}`, { limit: 10, ...filters }])
//
// The HTTP route lives in the queryKey argument, not in any recognizable HTTP call — there is no
// `fetch(...)`/`axios.*` call site at all in the calling file. zzop's native egress extractor is
// structurally blind to this: the route is data passed to a cache-key hook, not an argument to an
// HTTP client method. This adapter fills the gap WITHOUT teaching the engine react-query's vocabulary:
// it lexically matches `useQuery(`/`useInfiniteQuery(` call sites whose first queryKey element is a
// string/template literal, and projects each as an `IoConsume`, normalized to zzop's own
// `http_consume_interface_key` shape so the consume can join a native backend provide.
//
// USAGE
//   node adapter.mjs --root <frontend-root> [--source web] [--hooks useQuery,useInfiniteQuery] [--method GET]
// Writes the overlay envelope JSON to stdout; a one-line summary to stderr. Feed stdout to a tree's
// `adapterOverlays` array on an `analyze`/`analyzeTrees` request (see docs/NORMALIZED_AST.md).
//
// LIMITATIONS (intentional — a real adapter can go further): call detection is lexical and
// single-line (one call per matched line). Template `${...}` interpolation collapses to a single
// `{}` with no nesting support and no ternary fan-out — `` `/articles${filters.feed ? '/feed' : ''}` ``
// becomes the single key `/articles{}`, not two keys for the two branches. Only the v3 POSITIONAL-key
// idiom is covered (`useQuery('/x')` / `useQuery(['/x', vars])`); the object-form idiom
// (`useQuery({ queryKey: ['/x'], queryFn })`) is not matched. The emitted HTTP method is a flag
// (default `GET`) applied uniformly — react-query itself has no verb in the call site; the verb is
// whatever the app's default `queryFn` uses, which is app-specific and must be supplied by the caller.
import { readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';

function arg(name, def) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : def;
}

const feRoot = arg('root');
const source = arg('source', 'web');
const hooks = arg('hooks', 'useQuery,useInfiniteQuery')
  .split(',')
  .map((s) => s.trim())
  .filter(Boolean);
const method = arg('method', 'GET');
if (!feRoot) {
  console.error(
    'usage: node adapter.mjs --root <frontend-root> [--source web] [--hooks useQuery,useInfiniteQuery] [--method GET]'
  );
  process.exit(2);
}

const esc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const hookAlt = hooks.map(esc).join('|');
// `useQuery(` / `useInfiniteQuery(`, optionally an array-form queryKey opening `[`, then the first
// argument's leading quote. Group 1 = quote char, group 2 = literal text up to the matching unescaped
// quote (same "capture by backreference to the opening quote" trick as the openapi/wrapper adapters,
// which lets one pattern handle `'...'`, `"..."`, and `` `...` `` uniformly).
const callRe = new RegExp(
  `(?:^|[^.\\w$])(?:${hookAlt})\\s*\\(\\s*\\[?\\s*(['"\`])((?:\\\\.|(?!\\1).)*)\\1`
);

// Collapse every `${...}` interpolation in a template literal to `{}`. Non-greedy, no nesting: a
// `${...}` that itself contains `}` (e.g. an object literal argument) is not modeled — documented
// limitation, matches zzop's own `{param}` route-param normalization on the provide side.
function collapseTemplate(text) {
  return text.replace(/\$\{[^}]*\}/g, '{}');
}

// Normalize a raw queryKey literal into a zzop http-consume-interface-key path
// (`zzop_core::http_consume_interface_key` semantics — see packages/core/src/io.rs).
// Returns `null` for anything that should be skipped rather than guessed.
function normalize(literal) {
  // Skip conditions are checked on the literal BEFORE the leading-slash is auto-prepended below —
  // prepending first would make every case start with `/`, so "starts with {" (leading interpolation)
  // could never fire after the fact.
  if (/:\/\//.test(literal) || /\s/.test(literal)) return null; // external URL or non-path text
  if (literal.startsWith('.')) return null; // relative, not a route
  if (literal.startsWith('{')) return null; // leading interpolation — path itself is dynamic
  let p = literal.split(/[?#]/)[0]; // drop query/fragment suffix
  if (!p) return null; // empty after stripping query/fragment
  if (!p.startsWith('/')) p = `/${p}`; // root-normalize (queryKey literals are often host-relative, no leading slash)
  p = p.replace(/\/+/g, '/'); // collapse duplicate slashes
  if (p.length > 1) p = p.replace(/\/$/, ''); // drop trailing slash, keep bare "/"
  return p;
}

function walk(dir, out = []) {
  for (const e of readdirSync(dir)) {
    const abs = path.join(dir, e);
    const st = statSync(abs);
    if (st.isDirectory()) {
      if (e === 'node_modules' || e === '.git' || e === 'cypress') continue;
      walk(abs, out);
    } else if (/\.(ts|tsx|js|jsx|mjs)$/.test(e) && !/\.(spec|test)\.[tj]sx?$/.test(e)) {
      out.push(abs);
    }
  }
  return out;
}

const files = [];
let calls = 0;
let skipped = 0;
for (const abs of walk(feRoot)) {
  const text = readFileSync(abs, 'utf8');
  if (!hooks.some((h) => text.includes(h))) continue;
  const rel = path.relative(feRoot, abs).replace(/\\/g, '/');
  const lines = text.split('\n');
  const consumes = [];
  for (let i = 0; i < lines.length; i++) {
    const m = callRe.exec(lines[i]);
    if (!m) continue;
    calls++;
    const quote = m[1];
    const raw = quote === '`' ? collapseTemplate(m[2]) : m[2];
    const key = normalize(raw);
    if (key === null) {
      skipped++;
      continue;
    }
    consumes.push({ kind: 'http', key: `${method} ${key}`, file: rel, line: i + 1 });
  }
  if (consumes.length) files.push({ path: rel, loc: lines.length, io: { provides: [], consumes } });
}

process.stderr.write(
  `[react-query-adapter] ${files.length} file(s), ${calls} hook call site(s) matched, ${calls - skipped} keyed, ${skipped} skipped\n`
);
process.stdout.write(
  JSON.stringify({ format: 'zzop-normalized-ast', version: 1, parser: 'react-query-adapter/1', source, files })
);
