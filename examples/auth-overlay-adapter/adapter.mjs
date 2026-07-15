#!/usr/bin/env node
// Reference "Mode B" adapter for zzop: recognizes a project's router-level middleware auth
// registrations (an Express/Hono-style `app.use('/admin', requireAuth)`) and injects `auth-guarded`
// PathScope attributes through the generic entity-attribute channel — evidence the native call-graph
// BFS cannot see on its own.
//
// WHY THIS EXISTS
// `mutating-route-no-auth` (zzop_rules_http::mutating_route_no_auth) walks the call graph FROM a
// mutating route's handler, looking for a callee whose name looks like an auth guard. A router-level
// `.use(requireAuth)` middleware guards every route mounted under it WITHOUT ever being called BY the
// handler itself — it's a registration-time wiring fact, not a call edge — so it is structurally
// invisible to that BFS, and a route guarded only this way false-positives (Info severity, precisely
// because of this gap). Per zzop's direction (native sees the common in-body case; a per-project
// middleware CONVENTION is completed by injection rather than by ever teaching the engine every
// framework's middleware vocabulary natively), this adapter recognizes ONE common, concrete shape and
// injects its completion: `{ target: { pathScope: { prefix } }, key: "auth-guarded", value: true }`.
// The native BFS and this injected evidence COMPOSE — either one clears the route
// (`zzop_core::AttributeStore::route_attr`, checked before the BFS runs at all).
//
// USAGE:  node adapter.mjs --root <root> > auth.json
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { walk, EnvelopeBuilder } from '../adapter-kit/index.js';

function arg(name, def) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : def;
}
const root = arg('root');
if (!root) {
  console.error('usage: node adapter.mjs --root <root>');
  process.exit(2);
}

// Router-level middleware auth registration: `app.use('<prefix>', <guard>)` / `router.use("<prefix>", <guard>)`
// — a literal string prefix followed by a bare identifier guard argument. Only this common shape is
// recognized: a computed/templated prefix or an inline arrow-function guard is not (see README's
// Limitations). `g` flag so a single file can carry multiple registrations.
const USE_AUTH_RE = /\b(?:app|router)\.use\(\s*(['"])([^'"]+)\1\s*,\s*([A-Za-z_$][\w$]*)\s*\)/g;

// Guard-name vocabulary for the second `.use()` argument — matched against the whole identifier.
const GUARD_NAME_RE = /auth|guard|requireAuth|isAuthenticated/i;

const builder = new EnvelopeBuilder({ parser: 'auth-overlay-adapter/1', source: 'web' });
let fileCount = 0;
let registrationCount = 0;

// `walk` already returns repo-relative, forward-slash, lexically sorted paths — files are visited (and
// therefore emitted) in deterministic order with no extra sort needed here.
for (const rel of walk(root, { include: ['ts', 'js'] })) {
  const text = readFileSync(path.join(root, rel), 'utf8');

  const prefixes = new Set();
  for (const m of text.matchAll(USE_AUTH_RE)) {
    const [, , prefix, guardName] = m;
    if (GUARD_NAME_RE.test(guardName)) prefixes.add(prefix);
  }
  if (prefixes.size === 0) continue; // no auth-shaped registration in this file — omit it entirely

  // Deterministic ordering: registrations sorted by prefix within the file.
  const attributes = [...prefixes].sort().map((prefix) => ({
    target: { pathScope: { prefix } },
    key: 'auth-guarded',
    value: true,
  }));

  builder.addFile(rel, { loc: text.split('\n').length, attributes });
  fileCount++;
  registrationCount += attributes.length;
}

process.stderr.write(
  `[auth-overlay-adapter] ${fileCount} files, ${registrationCount} auth-guarded PathScope attributes injected\n`
);
process.stdout.write(JSON.stringify(builder.toEnvelope()));
