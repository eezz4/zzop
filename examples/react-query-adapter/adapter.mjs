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
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { walk, EnvelopeBuilder, resolveConsumeKey } from '../adapter-kit/index.js';

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

const builder = new EnvelopeBuilder({ parser: 'react-query-adapter/1', source });
let fileCount = 0;
let calls = 0;
let skipped = 0;
for (const rel of walk(feRoot, { include: ['ts', 'tsx', 'js', 'jsx', 'mjs'], excludeFile: /\.(spec|test)\.[tj]sx?$/, skipDirs: ['cypress'] })) {
  const text = readFileSync(path.join(feRoot, rel), 'utf8');
  if (!hooks.some((h) => text.includes(h))) continue;
  const lines = text.split('\n');
  const consumes = [];
  for (let i = 0; i < lines.length; i++) {
    const m = callRe.exec(lines[i]);
    if (!m) continue;
    calls++;
    const quote = m[1];
    const raw = quote === '`' ? collapseTemplate(m[2]) : m[2];
    // Delegated to adapter-kit's `resolveConsumeKey` — the same internal/external/base-relative
    // dispatch `parser/parser-typescript/src/adapters/egress.rs`'s `consume_key_for` uses: an
    // `http(s)://` literal keys VERBATIM as an external consume (never dropped — it still joins
    // `crossLayer.externalConsumes`), and a `:param` colon segment collapses to `{}` exactly like a
    // `{param}` template placeholder (`normalizeProvideKey`'s `RE_PARAM`). Returns the full
    // `"METHOD key"` string, or `null` when the literal clears no resolvable shape (reported via
    // `skipped`, never guessed).
    const key = resolveConsumeKey(method, raw);
    if (key === null) {
      skipped++;
      continue;
    }
    consumes.push({ key, line: i + 1 });
  }
  if (consumes.length) {
    builder.addFile(rel, { loc: lines.length });
    for (const c of consumes) builder.addConsume(rel, { kind: 'http', key: c.key, line: c.line });
    fileCount++;
  }
}

process.stderr.write(
  `[react-query-adapter] ${fileCount} file(s), ${calls} hook call site(s) matched, ${calls - skipped} keyed, ${skipped} skipped\n`
);
process.stdout.write(JSON.stringify(builder.toEnvelope()));
