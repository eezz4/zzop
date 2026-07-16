#!/usr/bin/env node
// Mode B adapter: projects react-query v3 positional queryKey call sites (`useQuery('/x')`,
// `useQuery(['/x', vars])`) as `IoConsume` facts in a NormalizedEnvelope overlay for zzop's
// `adapterOverlays` config, keyed to zzop's own `http_consume_interface_key` shape so a consume can
// join a native backend provide. Rationale and measured result: README.md.
//
// USAGE
//   node adapter.mjs --root <frontend-root> [--source web] [--hooks useQuery,useInfiniteQuery] [--method GET]
// Writes the overlay envelope JSON to stdout; a one-line summary to stderr. Feed stdout to a tree's
// `adapterOverlays` array on an `analyze`/`analyzeTrees` request (see docs/NORMALIZED_AST.md).
//
// CONTRACT CONSTRAINTS: call detection is lexical and single-line (one call per matched line).
// Template `${...}` interpolation collapses to a single `{}` — no nesting, no ternary fan-out. Only
// the v3 POSITIONAL-key idiom is matched (the object-form `useQuery({ queryKey: ['/x'], queryFn })`
// is not). The emitted HTTP method is the `--method` flag (default `GET`) applied uniformly — the
// call site carries no verb; it must be supplied by the caller, never guessed.
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
    // adapter-kit's `resolveConsumeKey` applies the same internal/external/base-relative dispatch
    // as the native extractor (`egress.rs`'s `consume_key_for`): an `http(s)://` literal keys
    // VERBATIM as an external consume, a `:param` segment collapses to `{}`. Returns the full
    // `"METHOD key"` string, or `null` when the literal clears no resolvable shape (counted as
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
