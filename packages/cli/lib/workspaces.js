'use strict';

// Workspace auto-detection for the `trees: "auto"` config shorthand.
//
// WHY THIS EXISTS: zzop's headline value (the cross-layer HTTP join) only fires when a repo is analyzed
// as MULTIPLE trees with distinct sourceIds. But that value is locked behind hand-writing a `trees: [...]`
// array — a monorepo user who just runs `zzop` gets a single mixed tree, which both external reviews
// flagged as a "discoverability" weakness (the join looks like a dead-code linter until you configure it).
// `trees: "auto"` expands, at run time, into one tree per workspace package — sourceId = the package's
// name — so a pnpm/npm/yarn workspace joins cross-layer with zero hand-authoring.
//
// DESIGN (deliberately conservative, reversible):
//   * OPT-IN ONLY. Nothing here runs unless `config.trees === "auto"` (an array or `roots` is untouched),
//     so no existing config changes behavior. `expandAutoTrees` is a pass-through for every other shape.
//   * PURE-ish: the only side effect is READING workspace manifests + `readdir` under the base dir. It
//     resolves everything against `baseDir` (the CLI passes `process.cwd()`, the same dir the native
//     engine resolves relative roots against — see bin/zzop.js) so a derived tree root means the same
//     thing to the engine as if the user had typed it.
//   * DEPENDENCY-FREE: this package ships with only @zzop/native as a dep, so there is no glob/YAML lib.
//     The pnpm-workspace.yaml reader is a MINIMAL parser for the common `packages:` list/flow forms, not a
//     general YAML parser; the directory glob supports `*`, `?`, `**`, and `!` negation — the vocabulary
//     pnpm/npm workspace globs actually use.
//
// Precedence when detecting the workspace package globs: pnpm-workspace.yaml wins (a pnpm repo may ALSO
// have a package.json without `workspaces`); otherwise package.json `workspaces` (npm/yarn). No workspace
// manifest at all is a ConfigError telling the user to write an explicit `trees` array or run from the
// workspace root — never a silent fallback to a single tree (that would re-hide the exact blindness this
// feature exists to remove).

const fs = require('node:fs');
const path = require('node:path');
const { ConfigError } = require('./mapper');

// Directories never descended into while expanding a `**` glob, and never returned as workspace packages:
// scanning them is both wasteful and wrong for workspace detection.
const SKIP_DIRS = new Set(['node_modules', '.git']);

// Hard cap on `**` recursion depth — a backstop against a pathological symlink cycle or an absurdly deep
// tree. Far below any real monorepo nesting.
const MAX_GLOB_DEPTH = 40;

/**
 * List immediate subdirectory names of `absDir`, excluding SKIP_DIRS. Returns [] for a missing/unreadable
 * directory (a glob pattern pointing at a non-existent path simply matches nothing).
 * @param {string} absDir
 * @returns {string[]}
 */
function listSubdirs(absDir) {
  let entries;
  try {
    entries = fs.readdirSync(absDir, { withFileTypes: true });
  } catch {
    return [];
  }
  const dirs = [];
  for (const e of entries) {
    if (!e.isDirectory()) continue;
    if (SKIP_DIRS.has(e.name)) continue;
    dirs.push(e.name);
  }
  return dirs;
}

/**
 * Compile a single glob path SEGMENT (no `/`) into an anchored RegExp. `*` matches any run of non-slash
 * chars, `?` matches one; every other regex metachar is escaped. Used only for `*`/`?` segments — literal
 * and `**` segments are handled directly by the caller.
 * @param {string} seg
 * @returns {RegExp}
 */
function segmentToRegExp(seg) {
  let re = '';
  for (const ch of seg) {
    if (ch === '*') re += '[^/]*';
    else if (ch === '?') re += '[^/]';
    else re += ch.replace(/[.+^${}()|[\]\\]/g, '\\$&');
  }
  return new RegExp(`^${re}$`);
}

/**
 * Expand one glob pattern's remaining `segments` from `currentRel` (a "/"-joined relative dir under
 * `baseDir`) into every matching relative directory path. Recursive; `**` matches zero or more directory
 * levels. Depth-guarded by MAX_GLOB_DEPTH.
 * @param {string} baseDir
 * @param {string} currentRel  relative dir already matched ("" for the base)
 * @param {string[]} segments  remaining pattern segments
 * @param {number} depth
 * @returns {string[]}
 */
function expandSegments(baseDir, currentRel, segments, depth) {
  if (segments.length === 0) return [currentRel];
  if (depth > MAX_GLOB_DEPTH) return [];

  const [seg, ...rest] = segments;
  const currentAbs = path.join(baseDir, currentRel);
  const results = [];

  if (seg === '**') {
    // Zero levels: apply the rest right here. One-or-more: recurse into each subdir keeping `**`.
    results.push(...expandSegments(baseDir, currentRel, rest, depth));
    for (const sub of listSubdirs(currentAbs)) {
      const nextRel = currentRel ? `${currentRel}/${sub}` : sub;
      results.push(...expandSegments(baseDir, nextRel, segments, depth + 1));
    }
    return results;
  }

  const isWild = /[*?]/.test(seg);
  const matcher = isWild ? segmentToRegExp(seg) : null;
  for (const sub of listSubdirs(currentAbs)) {
    if (isWild ? matcher.test(sub) : sub === seg) {
      const nextRel = currentRel ? `${currentRel}/${sub}` : sub;
      results.push(...expandSegments(baseDir, nextRel, rest, depth + 1));
    }
  }
  return results;
}

/**
 * Resolve workspace-package glob patterns to the set of relative directories that both match a positive
 * pattern, survive every `!`-negated pattern, and contain a package.json. Deterministically sorted.
 * @param {string} baseDir
 * @param {string[]} patterns  e.g. ["packages/*", "apps/*", "!**\/examples/**"]
 * @returns {string[]}  relative dir paths (forward slashes), sorted
 */
function resolveWorkspaceDirs(baseDir, patterns) {
  const positives = [];
  const negatives = [];
  for (const raw of patterns) {
    if (typeof raw !== 'string' || raw.trim() === '') continue;
    const p = raw.trim();
    if (p.startsWith('!')) negatives.push(p.slice(1));
    else positives.push(p);
  }

  const matched = new Set();
  for (const pattern of positives) {
    const segments = pattern.split('/').filter((s) => s !== '' && s !== '.');
    for (const rel of expandSegments(baseDir, '', segments, 0)) {
      if (rel !== '') matched.add(rel);
    }
  }

  const negRegexes = negatives.map(globToFullRegExp);
  const kept = [];
  for (const rel of matched) {
    if (negRegexes.some((re) => re.test(rel))) continue;
    if (fs.existsSync(path.join(baseDir, rel, 'package.json'))) kept.push(rel);
  }
  kept.sort();
  return kept;
}

/**
 * Compile a WHOLE glob path (may contain `/` and `**`) into an anchored RegExp for negation matching.
 * `**` matches across directory separators; `*`/`?` do not. Only used for `!`-negations, where matching a
 * full relative path (not walking the tree) is what we want.
 * @param {string} pattern
 * @returns {RegExp}
 */
function globToFullRegExp(pattern) {
  const segs = pattern.split('/').filter((s) => s !== '' && s !== '.');
  let re = '';
  segs.forEach((seg, i) => {
    if (i > 0) re += '/';
    if (seg === '**') {
      // `**` as a segment: any number of path chars including separators (and it may stand in for the
      // separator it introduced, so allow the preceding "/" to be absorbed).
      re = re.replace(/\/$/, '(?:/.*)?');
    } else {
      for (const ch of seg) {
        if (ch === '*') re += '[^/]*';
        else if (ch === '?') re += '[^/]';
        else re += ch.replace(/[.+^${}()|[\]\\]/g, '\\$&');
      }
    }
  });
  return new RegExp(`^${re}$`);
}

/**
 * Strip one layer of matching surrounding single/double quotes from a scalar.
 * @param {string} s
 * @returns {string}
 */
function unquote(s) {
  const t = s.trim();
  if (t.length >= 2 && ((t[0] === '"' && t[t.length - 1] === '"') || (t[0] === "'" && t[t.length - 1] === "'"))) {
    return t.slice(1, -1);
  }
  return t;
}

/**
 * MINIMAL pnpm-workspace.yaml reader: returns the `packages:` glob list, or null if the file is absent.
 * Supports the two forms real pnpm workspaces use — a block list:
 *   packages:
 *     - 'packages/*'
 *     - "apps/*"
 * and an inline flow list: `packages: ['packages/*', 'apps/*']`. Comments (`#`) and blank lines are
 * ignored. This is NOT a general YAML parser; an exotic file that doesn't match these forms yields an
 * empty list (surfaced to the user as "no packages found", not a crash).
 * @param {string} baseDir
 * @returns {string[]|null}
 */
function readPnpmWorkspacePackages(baseDir) {
  const file = path.join(baseDir, 'pnpm-workspace.yaml');
  let raw;
  try {
    raw = fs.readFileSync(file, 'utf8');
  } catch {
    return null;
  }

  const lines = raw.split(/\r?\n/);
  const patterns = [];
  let inPackages = false;
  for (const line of lines) {
    const noComment = line.replace(/\s+#.*$/, '');
    // Top-level `packages:` key (no leading indentation).
    const keyMatch = /^packages\s*:(.*)$/.exec(noComment);
    if (keyMatch) {
      const inline = keyMatch[1].trim();
      if (inline.startsWith('[')) {
        // Flow list on one line: packages: ['a', 'b']
        const inner = inline.replace(/^\[/, '').replace(/\]\s*$/, '');
        for (const part of inner.split(',')) {
          const v = unquote(part);
          if (v !== '') patterns.push(v);
        }
        inPackages = false;
      } else {
        inPackages = true;
      }
      continue;
    }
    if (!inPackages) continue;
    // A block-list item: `  - 'packages/*'`
    const itemMatch = /^\s*-\s*(.+?)\s*$/.exec(noComment);
    if (itemMatch) {
      const v = unquote(itemMatch[1]);
      if (v !== '') patterns.push(v);
      continue;
    }
    // A blank/comment-only line stays inside the block; anything else (a new top-level key) ends it.
    if (noComment.trim() === '') continue;
    if (/^\S/.test(noComment)) inPackages = false;
  }
  return patterns;
}

/**
 * npm/yarn workspace globs from a package.json `workspaces` field (array, or `{ packages: [...] }`).
 * Returns null if there is no package.json or no `workspaces` field.
 * @param {string} baseDir
 * @returns {string[]|null}
 */
function readNpmWorkspacePackages(baseDir) {
  const file = path.join(baseDir, 'package.json');
  let raw;
  try {
    raw = fs.readFileSync(file, 'utf8');
  } catch {
    return null;
  }
  let pkg;
  try {
    pkg = JSON.parse(raw);
  } catch {
    return null;
  }
  const ws = pkg && pkg.workspaces;
  if (Array.isArray(ws)) return ws.filter((p) => typeof p === 'string');
  if (ws && typeof ws === 'object' && Array.isArray(ws.packages)) {
    return ws.packages.filter((p) => typeof p === 'string');
  }
  return null;
}

/**
 * Read a workspace package's declared name from its package.json, or null when absent/unnamed — the caller
 * falls back to the relative directory path so a nameless package still gets a distinct sourceId.
 * @param {string} pkgDirAbs
 * @returns {string|null}
 */
function readPackageName(pkgDirAbs) {
  try {
    const pkg = JSON.parse(fs.readFileSync(path.join(pkgDirAbs, 'package.json'), 'utf8'));
    return pkg && typeof pkg.name === 'string' && pkg.name !== '' ? pkg.name : null;
  } catch {
    return null;
  }
}

/**
 * Expand a `trees: "auto"` config into a concrete `trees: [{ root, sourceId }]` array by detecting the
 * workspace's packages under `baseDir`. Any config whose `trees` is not exactly the string "auto" is
 * returned UNCHANGED (this is the no-op pass-through for the array/`roots`/omitted shapes).
 *
 * Throws ConfigError when `trees: "auto"` is set but no workspace manifest is found or it yields no
 * packages — never silently degrades to a single tree.
 *
 * @param {object} config  parsed config object
 * @param {string} baseDir  directory to resolve the workspace against (the CLI passes process.cwd())
 * @returns {{ config: object, warnings: string[] }}  a new config (trees replaced) + human-readable notes
 */
function expandAutoTrees(config, baseDir) {
  if (!config || typeof config !== 'object' || Array.isArray(config) || config.trees !== 'auto') {
    return { config, warnings: [] };
  }

  let patterns = readPnpmWorkspacePackages(baseDir);
  let source = 'pnpm-workspace.yaml';
  if (patterns === null) {
    patterns = readNpmWorkspacePackages(baseDir);
    source = 'package.json "workspaces"';
  }
  if (patterns === null) {
    throw new ConfigError(
      `trees: "auto" found no workspace manifest in ${baseDir} — expected a pnpm-workspace.yaml with a ` +
        `"packages:" list, or a package.json with a "workspaces" field. Write an explicit ` +
        `"trees": [{ "root": ..., "sourceId": ... }] array instead, or run zzop from the workspace root.`
    );
  }

  const dirs = resolveWorkspaceDirs(baseDir, patterns);
  if (dirs.length === 0) {
    throw new ConfigError(
      `trees: "auto" matched no package directories from ${source} (patterns: ${patterns.join(', ') || '(none)'}). ` +
        `Each pattern must resolve to directories containing a package.json. Write an explicit "trees" array instead.`
    );
  }

  const warnings = [];
  const seenSource = new Map(); // sourceId -> first root, to detect collisions
  const trees = dirs.map((rel) => {
    const name = readPackageName(path.join(baseDir, rel));
    const sourceId = name || rel;
    if (seenSource.has(sourceId)) {
      warnings.push(
        `trees: "auto" derived a duplicate sourceId "${sourceId}" for both "${seenSource.get(sourceId)}" and ` +
          `"${rel}". Cross-source joins key on sourceId; give one package a distinct "name" or use an explicit ` +
          `"trees" array to disambiguate.`
      );
    } else {
      seenSource.set(sourceId, rel);
    }
    return { root: rel, sourceId };
  });

  warnings.push(
    `trees: "auto" expanded to ${trees.length} tree(s) from ${source}: ` +
      `${trees.map((t) => `${t.sourceId} (${t.root})`).join(', ')}.`
  );
  if (trees.length === 1) {
    warnings.push(
      `trees: "auto" resolved only one workspace package — the cross-layer join needs >= 2 trees with ` +
        `distinct sourceIds to fire, so this run behaves like a single-tree analysis.`
    );
  }

  // Replace `trees` with the concrete array; preserve every other key (overlays, packs, rules, git, ...).
  // Shadowed-key honesty (parity with crates/config's expand_auto_trees): `roots` never steers the auto
  // scan — warn and strip it so the inert key can't silently look load-bearing downstream.
  const expanded = { ...config, trees };
  if (config.roots !== undefined) {
    warnings.push(
      'config has both "roots" and "trees": "auto" — auto wins and scans the config file\'s directory ' +
        'for workspace members; "roots" is ignored in auto mode (remove one).'
    );
    delete expanded.roots;
  }
  return { config: expanded, warnings };
}

module.exports = { expandAutoTrees };
