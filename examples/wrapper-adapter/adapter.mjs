#!/usr/bin/env node
// Mode B adapter: projects a hand-rolled HTTP client wrapper's call sites
// (`<wrapper>.<verb>(<pathLiteral>)`, e.g. `requests.get('/articles')`) as `IoConsume` facts in a
// NormalizedEnvelope overlay for zzop's `adapterOverlays` config, keyed the same way zzop keys
// routes so a consume can join a native backend provide. Rationale and measured result: README.md.
//
// USAGE
//   node adapter.mjs --root <feRoot> [--wrapper requests,agent,api] [--source <id>]
// Writes the overlay envelope JSON to stdout; a one-line summary to stderr. Feed stdout to a tree's
// `adapterOverlays` array on an `analyze`/`analyzeTrees` request (see docs/NORMALIZED_AST.md).
//
// CONTRACT CONSTRAINTS: the wrapper binding must be named (default
// `requests`/`agent`/`api`/`http`/`client`); only a first-argument STRING literal path is keyed (a
// path built in a variable or concatenated is skipped — never guessed); template `${...}` and
// `:param` segments normalize to `{}` (zzop's own route-param key), a `?query`/`#fragment` suffix
// is dropped; call detection is lexical and single-line. A path with no leading `/` resolves as
// base-relative (the axios/ky `baseURL` idiom) the same way native egress extraction does — see
// `resolveConsumeKey`.
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { walk, EnvelopeBuilder, resolveConsumeKey } from '../adapter-kit/index.js';

function arg(name, def) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : def;
}

const feRoot = arg('root');
const source = arg('source', 'web');
const wrappers = arg('wrapper', 'requests,agent,api,http,client')
  .split(',')
  .map((s) => s.trim())
  .filter(Boolean);
if (!feRoot) {
  console.error('usage: node adapter.mjs --root <feRoot> [--wrapper requests,agent] [--source <id>]');
  process.exit(2);
}

// superagent's `del` and standard `delete` both map to DELETE. `delete` is listed before `del` so the
// alternation prefers the longer match on a `.delete(` call site.
const VERB = { get: 'GET', post: 'POST', put: 'PUT', patch: 'PATCH', delete: 'DELETE', del: 'DELETE', head: 'HEAD' };
const esc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const wrapperAlt = wrappers.map(esc).join('|');
const verbAlt = Object.keys(VERB).join('|');
// `<wrapper>.<verb>( '<literal>'` — capture the receiver (1), verb (2), quote (3), path text (4). The
// leading `(?:^|[^.\w$])` stops `foo.requests.get(` / `myrequests.get(` from matching a bare `requests`.
const callRe = new RegExp(
  `(?:^|[^.\\w$])(${wrapperAlt})\\.(${verbAlt})\\s*\\(\\s*(['"\`])((?:\\\\.|(?!\\3).)*)\\3`
);

// Collapse a JS template literal's `${...}` interpolations to zzop's own `{}` route-param placeholder
// BEFORE handing the literal to adapter-kit's `resolveConsumeKey`: the kit's normalization mirrors
// `zzop_core::http_interface_key`, which only ever sees a `{param}`/`:param`-SHAPED route pattern —
// never raw JS template syntax. Non-greedy, no nesting — same documented limitation as the
// react-query adapter's `collapseTemplate`.
function collapseTemplate(raw) {
  return raw.replace(/\$\{[^}]*\}/g, '{}');
}

const builder = new EnvelopeBuilder({ parser: 'wrapper-adapter/1', source });
let fileCount = 0;
let calls = 0;
let skipped = 0;
for (const rel of walk(feRoot, { include: ['ts', 'tsx', 'js', 'jsx', 'mjs'], excludeFile: /\.(spec|test)\.[tj]sx?$/ })) {
  const text = readFileSync(path.join(feRoot, rel), 'utf8');
  if (!wrappers.some((w) => text.includes(`${w}.`))) continue;
  const lines = text.split('\n');
  const consumes = [];
  for (let i = 0; i < lines.length; i++) {
    const m = callRe.exec(lines[i]);
    if (!m) continue;
    calls++;
    const rawPath = collapseTemplate(m[4]);
    // adapter-kit's `resolveConsumeKey` applies the same internal/external/base-relative dispatch
    // as the native extractor (`egress.rs`'s `consume_key_for`): a leading `/` keys directly,
    // `http(s)://` keys verbatim as an external consume, a bare `path/like/this` resolves as
    // base-relative (the axios/ky `baseURL` idiom), and a `?query`/`#fragment` suffix drops on
    // either resolved path. Returns the full `"METHOD key"` string, or `null` when the literal
    // clears no resolvable shape (counted as `skipped`, never guessed).
    const key = resolveConsumeKey(VERB[m[2]], rawPath);
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
  `[wrapper-adapter] ${fileCount} file(s), ${calls} wrapper call site(s) matched, ${calls - skipped} keyed, ${skipped} skipped\n`
);
process.stdout.write(JSON.stringify(builder.toEnvelope()));
