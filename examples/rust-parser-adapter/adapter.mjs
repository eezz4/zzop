#!/usr/bin/env node
// Reference "Mode A" parser for zzop: a lexical Rust parser that projects a Rust workspace into a
// complete NormalizedEnvelope for `analyzeEnvelope` (docs/NORMALIZED_AST.md).
//
// WHY THIS EXISTS
// zzop's engine has no Rust crate — Rust enters (like any new language) through the external-parser
// contract: an out-of-process producer emits one `NormalizedEnvelope` and the engine runs every
// language-neutral analysis it can on the projected channels (dep graph -> circular / dead-candidates /
// unreachable / fan-in-out / folder rollups, symbols -> symbol-scan DSL rules, coverage census,
// config diagnostics). This adapter is the first *runnable* Mode A reference (the JSP example is a
// hand-written fixture) — and its worked demo is zzop analyzing its own Rust workspace.
//
// WHAT IT PROJECTS (all lexical, line-based — no real Rust parse)
// - symbols:  top-level `fn` / `struct` / `enum` / `trait` / `type` / `const` / `static` items;
//   `exported` = declared with any `pub` form (`pub`, `pub(crate)`, `pub(super)`, `pub(in ...)`).
// - imports:  `mod foo;` declarations edge to the child module file (`foo.rs` / `foo/mod.rs`), and
//   `use` paths (`crate::`, `super::`, `self::`, and cross-crate `use other_workspace_crate::...`)
//   resolve module-tree prefixes to repo-relative file paths. The envelope contract matches import
//   specifiers against the envelope's own path set EXACTLY, so the adapter resolves paths itself; an
//   unresolvable path is simply omitted (external, never guessed).
// - is_entry: crate roots and cargo-convention files (`lib.rs`, `main.rs`, `build.rs`, `tests/**`,
//   `benches/**`, `examples/**`, `src/bin/**`) — cargo loads these by convention with zero in-repo
//   importers, exactly what the contract's `is_entry` marker exists for.
// - io:       left empty on purpose. This adapter models the module graph, not routes; a Rust web
//   service would extract its axum/actix routes here the same way (see the Mode B adapters).
//
// USAGE
//   node adapter.mjs --root <workspaceRoot> [--source <id>]
// Writes the envelope JSON to stdout; a one-line summary to stderr. Feed stdout to
// `analyzeEnvelope(envelopeJson, configJson)` — `analyze.mjs` next to this file does exactly that.
//
// LIMITATIONS (intentional — a real adapter can go further): detection is lexical and line-based, so
// items inside `macro_rules!`/`cfg`-gated blocks or multi-line `use` groups spanning `{` newlines are
// approximated (grouped `use a::{b, c}` on ONE line is handled); `#[path = ...]` module overrides,
// `include!`, and re-export chains (`pub use`) are not modeled; glob imports resolve to the module
// file itself (which is the right dep edge anyway).
import { readFileSync, readdirSync, statSync, existsSync } from 'node:fs';
import path from 'node:path';

function arg(name, def) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : def;
}

const root = arg('root');
const source = arg('source', 'rust');
if (!root) {
  console.error('usage: node adapter.mjs --root <workspaceRoot> [--source <id>]');
  process.exit(2);
}

// --- walk: every .rs file, skipping build output and vendored trees ---------------------------------
const SKIP_DIRS = new Set(['target', 'node_modules', '.git', '.zzop-cache']);
function walk(dir, out = []) {
  for (const e of readdirSync(dir)) {
    if (SKIP_DIRS.has(e)) continue;
    const abs = path.join(dir, e);
    const st = statSync(abs);
    if (st.isDirectory()) walk(abs, out);
    else if (e.endsWith('.rs')) out.push(abs);
  }
  return out;
}

const absFiles = walk(root);
const rels = absFiles.map((abs) => path.relative(root, abs).replace(/\\/g, '/'));
const relSet = new Set(rels);

// --- crate map: nearest-ancestor Cargo.toml gives each file its crate; `[package] name` (with `-`
// normalized to `_`, as rustc does) lets `use other_crate::...` resolve across the workspace ---------
const crateSrcByName = new Map(); // crate_name -> src dir rel (e.g. "packages/core/src")
const crateSrcDirs = []; // sorted longest-first for nearest-ancestor lookup
const manifestEntries = new Set(); // explicit cargo target files: [[test]]/[[bin]]/[lib] `path = "x.rs"`
(function findCrates(dir) {
  for (const e of readdirSync(dir)) {
    if (SKIP_DIRS.has(e)) continue;
    const abs = path.join(dir, e);
    if (statSync(abs).isDirectory()) findCrates(abs);
    else if (e === 'Cargo.toml') {
      const text = readFileSync(abs, 'utf8');
      const dirRel = path.relative(root, dir).replace(/\\/g, '/');
      const name = /^\s*name\s*=\s*"([^"]+)"/m.exec(text);
      const srcRel = dirRel ? `${dirRel}/src` : 'src';
      if (name && existsSync(path.join(root, srcRel))) {
        crateSrcByName.set(name[1].replace(/-/g, '_'), srcRel);
        crateSrcDirs.push(srcRel);
      }
      // Explicit target files (`[[test]] path = "dsl/http/http.rs"`, custom `[lib]`/`[[bin]]` paths):
      // cargo loads these directly by manifest, so they are entry files with zero in-repo importers.
      for (const m of text.matchAll(/^\s*path\s*=\s*"([^"]+\.rs)"/gm)) {
        manifestEntries.add(dirRel ? `${dirRel}/${m[1]}` : m[1]);
      }
    }
  }
})(root);
crateSrcDirs.sort((a, b) => b.length - a.length);

function crateSrcOf(rel) {
  return crateSrcDirs.find((src) => rel === src || rel.startsWith(`${src}/`));
}

// Resolve one `::`-separated module path from a starting directory: descend while a segment names a
// directory module (`seg/mod.rs`) and record the deepest FILE the prefix reaches (`seg.rs` stops the
// walk — deeper segments are items or inline modules living in that same file).
function resolveModPath(startDirRel, segments) {
  let dir = startDirRel;
  let target = null;
  for (const seg of segments) {
    if (relSet.has(`${dir}/${seg}.rs`)) {
      target = `${dir}/${seg}.rs`;
      break;
    }
    if (relSet.has(`${dir}/${seg}/mod.rs`)) {
      target = `${dir}/${seg}/mod.rs`;
      dir = `${dir}/${seg}`;
      continue;
    }
    break;
  }
  return target;
}

// The directory whose children are this file's submodules: `a/b.rs` and `a/b/mod.rs` both own `a/b/`,
// while a crate root (`lib.rs`/`main.rs`) or `mod.rs` owns its containing directory.
function moduleDirOf(rel) {
  const base = path.posix.basename(rel);
  if (base === 'mod.rs' || base === 'lib.rs' || base === 'main.rs') return path.posix.dirname(rel);
  return rel.slice(0, -'.rs'.length);
}

const ITEM_RE =
  /^\s*(pub(?:\((?:crate|super|self|in\s+[^)]*)\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+"[^"]*"\s+)?(fn|struct|enum|trait|type|const|static)\s+([A-Za-z_]\w*)/;
const KIND = { fn: 'function', struct: 'class', enum: 'type', trait: 'interface', type: 'type', const: 'const', static: 'const' };
const MOD_RE = /^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_]\w*)\s*;/;
const USE_RE = /^\s*(?:pub\s+)?use\s+([A-Za-z_][\w:]*(?:::\{[^}]*\})?)/;

function entryFile(rel) {
  const base = path.posix.basename(rel);
  return (
    base === 'lib.rs' ||
    base === 'main.rs' ||
    base === 'build.rs' ||
    /(^|\/)(tests|benches|examples)\//.test(rel) ||
    /(^|\/)src\/bin\//.test(rel) ||
    manifestEntries.has(rel)
  );
}

const files = [];
let totalSymbols = 0;
let totalImports = 0;
for (let f = 0; f < absFiles.length; f++) {
  const rel = rels[f];
  const text = readFileSync(absFiles[f], 'utf8');
  const lines = text.split('\n');
  const crateSrc = crateSrcOf(rel);
  const modDir = moduleDirOf(rel);

  const symbols = [];
  const imports = {};
  // `typeOnly` marks an edge with no load semantics for cycle purposes: when a `use` names an ITEM at a
  // crate root (`use crate::RuleRegistry`) we fall back to the root FILE — a real fan-in edge, but
  // counting it as a cycle edge would manufacture a `root <-> module` 2-cycle out of every root-item
  // import (the root `mod`-declares the module back). Rust has no module load order, so excluding the
  // approximated edge from cycle detection is the honest projection; genuine sibling `use` edges stay
  // cycle-eligible (mutual module coupling IS a real signal).
  const addImport = (local, specifier, original, typeOnly = false) => {
    if (specifier && specifier !== rel && !(local in imports)) {
      imports[local] = { specifier, original, ...(typeOnly ? { type_only: true } : {}) };
      totalImports++;
    }
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    const item = ITEM_RE.exec(line);
    if (item) {
      symbols.push({
        id: `${rel}#${item[3]}`,
        file: rel,
        name: item[3],
        kind: KIND[item[2]],
        line: i + 1,
        exported: Boolean(item[1]),
      });
      continue;
    }

    const mod = MOD_RE.exec(line);
    if (mod) {
      // `mod foo;` loads a child file — the strongest, unambiguous dep edge in the module tree.
      const target = relSet.has(`${modDir}/${mod[1]}.rs`)
        ? `${modDir}/${mod[1]}.rs`
        : relSet.has(`${modDir}/${mod[1]}/mod.rs`)
          ? `${modDir}/${mod[1]}/mod.rs`
          : null;
      addImport(`mod:${mod[1]}`, target, mod[1]);
      continue;
    }

    const use = USE_RE.exec(line);
    if (!use) continue;
    // Expand a single-line group `a::b::{c, d::e}` into one path per member; a plain path is itself.
    const raw = use[1];
    const group = /^(.*)::\{([^}]*)\}$/.exec(raw);
    const paths = group
      ? group[2].split(',').map((m) => `${group[1]}::${m.trim().split(/\s+as\s+/)[0].trim()}`).filter((p) => !p.endsWith('::'))
      : [raw.split(/\s+as\s+/)[0]];
    for (const p of paths) {
      resolveAndAdd(p);
    }
  }

  // Inline qualified paths (`zzop_git::collect(...)`, `crate::analyze::assemble(...)`) need no `use`
  // in Rust 2018+ — without this pass a crate referenced only through inline paths reads as an
  // unreachable island. Lexical: string/comment contents can match (documented limitation); the
  // lowercase-head requirement skips `String::new` / enum variants, and unknown heads (std, external
  // registry crates) resolve to null and are dropped.
  const INLINE_RE = /(?:^|[^\w:.$"'])((?:crate|[a-z_][a-z0-9_]*))::([A-Za-z_]\w*(?:::[A-Za-z_]\w*)*)/g;
  for (const line of lines) {
    if (/^\s*(?:\/\/|use\s|mod\s)/.test(line)) continue;
    for (const m of line.matchAll(INLINE_RE)) {
      const head = m[1];
      if (head !== 'crate' && !crateSrcByName.has(head)) continue;
      resolveAndAdd(`${head}::${m[2]}`);
    }
  }

  function resolveAndAdd(p) {
    const segs = p.split('::').filter((s) => s && s !== '*');
    if (segs.length === 0) return;
    const head = segs[0];
    let target = null;
    let rootFallback = null;
    if (head === 'crate' && crateSrc) {
      target = resolveModPath(crateSrc, segs.slice(1));
      rootFallback = relSet.has(`${crateSrc}/lib.rs`) ? `${crateSrc}/lib.rs` : null;
    } else if (head === 'self') {
      target = resolveModPath(modDir, segs.slice(1));
    } else if (head === 'super') {
      // Strip leading `super`s, walking up one module level each (module level ~= one path segment).
      let dir = path.posix.dirname(modDir);
      let rest = segs.slice(1);
      while (rest[0] === 'super') {
        dir = path.posix.dirname(dir);
        rest = rest.slice(1);
      }
      target =
        resolveModPath(dir, rest) ||
        (relSet.has(`${dir}/mod.rs`) ? `${dir}/mod.rs` : relSet.has(`${dir}.rs`) ? `${dir}.rs` : null);
    } else if (crateSrcByName.has(head)) {
      const src = crateSrcByName.get(head);
      target = resolveModPath(src, segs.slice(1));
      rootFallback = relSet.has(`${src}/lib.rs`) ? `${src}/lib.rs` : null;
    }
    // std / external-registry crates fall through with target=null — external, never an error. A path
    // that names a root ITEM (no module file matched) edges to the crate root file as `type_only`.
    if (target) addImport(p, target, segs[segs.length - 1]);
    else if (rootFallback) addImport(p, rootFallback, segs[segs.length - 1], true);
  }

  totalSymbols += symbols.length;
  files.push({
    path: rel,
    loc: lines.length,
    symbols,
    imports,
    ...(entryFile(rel) ? { is_entry: true } : {}),
  });
}

process.stderr.write(
  `[rust-parser-adapter] ${files.length} file(s), ${totalSymbols} symbol(s), ${totalImports} import binding(s), ${crateSrcByName.size} crate(s)\n`
);
process.stdout.write(
  JSON.stringify({ format: 'zzop-normalized-ast', version: 1, parser: 'rust-lexical/1', source, files })
);
