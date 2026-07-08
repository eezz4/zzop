#!/usr/bin/env node
// Reference "Mode B" adapter for zzop: resolve a hand-rolled HTTP client wrapper's call sites into
// cross-layer IO facts, emitted as a NormalizedEnvelope overlay for zzop's `adapterOverlays` config.
//
// WHY THIS EXISTS
// Many frontends never call `fetch`/`axios` directly — they funnel every request through one central
// hand-written wrapper module (a superagent/got/node-fetch-style helper), e.g.:
//
//     const requests = {
//       get:  url        => superagent.get(`${API_ROOT}${url}`).then(...),
//       post: (url, body) => superagent.post(`${API_ROOT}${url}`, body).then(...),
//       del:  url        => superagent.del(`${API_ROOT}${url}`).then(...),
//     };
//     // ...callers everywhere:
//     requests.get('/articles');
//     requests.post('/users/login', { user });
//     requests.get(`/articles/${slug}/comments`);
//
// zzop's native egress extractor recognizes `fetch`/`axios`/`ky` (and a few generated-client runtimes),
// but the route here lives at the WRAPPER call site (`requests.get('/articles')`), keyed by a verb-named
// method plus a path argument the engine does not model. So the whole tree goes SILENT — not "0 findings"
// but "extracted nothing", which is the worst cross-layer failure (a reviewing agent can't tell a clean
// tree from a blind one). This adapter fills the gap WITHOUT teaching the engine the wrapper's vocabulary:
// it lexically matches `<wrapper>.<verb>(<pathLiteral>)` call sites and projects each as an `IoConsume`,
// normalizing the path the same way zzop keys routes so the consume JOINS a native backend provide.
//
// USAGE
//   node adapter.mjs --root <feRoot> [--wrapper requests,agent,api] [--source <id>]
// Writes the overlay envelope JSON to stdout; a one-line summary to stderr. Feed stdout to a tree's
// `adapterOverlays` array on an `analyze`/`analyzeTrees` request (see docs/NORMALIZED_AST.md).
//
// LIMITATIONS (intentional — a real adapter can go further): the wrapper binding must be named (default
// `requests`/`agent`/`api`/`http`/`client`); only a first-argument STRING literal path is keyed (a path
// built in a variable, concatenated, or with a leading `${...}` host is skipped — never guessed); template
// `${...}` and `:param` segments normalize to `{}` (zzop's own route-param key), query strings are dropped;
// call detection is lexical and single-line.
import { readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';

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

// Normalize a raw path literal into a zzop http-interface key path: drop the query string, template
// `${...}` and `:param` -> `{}`, collapse `//`, trim a trailing `/`.
function normPath(raw) {
  let p = raw.split('?')[0];
  p = p.replace(/\$\{[^}]*\}/g, '{}').replace(/:[A-Za-z_$][\w$]*/g, '{}');
  p = p.replace(/\/+/g, '/').replace(/\/$/, '');
  return p || '/';
}

function walk(dir, out = []) {
  for (const e of readdirSync(dir)) {
    const abs = path.join(dir, e);
    const st = statSync(abs);
    if (st.isDirectory()) {
      if (e === 'node_modules' || e === '.git') continue;
      walk(abs, out);
    } else if (/\.(ts|tsx|js|jsx|mjs)$/.test(e) && !/\.(spec|test)\.[tj]sx?$/.test(e)) {
      out.push(abs);
    }
  }
  return out;
}

const files = [];
let calls = 0;
for (const abs of walk(feRoot)) {
  const text = readFileSync(abs, 'utf8');
  if (!wrappers.some((w) => text.includes(`${w}.`))) continue;
  const rel = path.relative(feRoot, abs).replace(/\\/g, '/');
  const lines = text.split('\n');
  const consumes = [];
  for (let i = 0; i < lines.length; i++) {
    const m = callRe.exec(lines[i]);
    if (!m) continue;
    const rawPath = m[4];
    // Only a literal path rooted at `/` is a route we can key. A `${API_ROOT}${url}`-style argument
    // (host built at the call site) or a non-path string is left for native egress / a richer adapter.
    if (!rawPath.startsWith('/')) continue;
    consumes.push({ kind: 'http', key: `${VERB[m[2]]} ${normPath(rawPath)}`, file: rel, line: i + 1 });
    calls++;
  }
  if (consumes.length) files.push({ path: rel, loc: lines.length, io: { provides: [], consumes } });
}

process.stderr.write(`[wrapper-adapter] ${files.length} file(s), ${calls} wrapper call site(s) keyed\n`);
process.stdout.write(
  JSON.stringify({ format: 'zzop-normalized-ast', version: 1, parser: 'wrapper-adapter', source, files })
);
