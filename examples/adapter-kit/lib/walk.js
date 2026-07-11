// Deterministic recursive file walk for adapters. Every existing example adapter in this repo
// (openapi-sdk-adapter, react-query-adapter, wrapper-adapter, svelte-adapter, rust-parser-adapter)
// hand-rolls a near-identical recursive walk that skips `node_modules`/`.git` and filters by
// extension — this is that boilerplate, extracted once.

import { readdirSync } from 'node:fs';
import path from 'node:path';

const DEFAULT_SKIP_DIRS = new Set(['node_modules', '.git']);

function normalizeExt(ext) {
  return ext.startsWith('.') ? ext : `.${ext}`;
}

/**
 * Recursively lists files under `root`, returned as repo-relative, forward-slash, LEXICALLY SORTED
 * paths — deterministic regardless of the underlying filesystem's directory-entry order (OS readdir
 * order is not guaranteed stable across platforms or even repeated calls).
 *
 * @param {string} root - Absolute or relative directory to walk.
 * @param {object} [options]
 * @param {string[]} [options.include] - Only these extensions (`.ts`/`ts` both accepted). Omit for
 *   "every extension".
 * @param {string[]} [options.exclude] - Never these extensions, checked before `include`.
 * @param {string[]} [options.skipDirs] - Additional directory NAMES to skip, on top of the always-on
 *   `node_modules`/`.git`.
 * @param {RegExp} [options.excludeFile] - Skip a file whose NAME (not full path) matches this pattern
 *   (e.g. `/\.(spec|test)\.[tj]sx?$/` to skip test files, as every example adapter does inline).
 * @returns {string[]} Sorted, repo-relative, forward-slash file paths.
 */
export function walk(root, options = {}) {
  const include = options.include ? new Set(options.include.map(normalizeExt)) : null;
  const exclude = options.exclude ? new Set(options.exclude.map(normalizeExt)) : new Set();
  const skipDirs = new Set([...DEFAULT_SKIP_DIRS, ...(options.skipDirs || [])]);
  const excludeFile = options.excludeFile;

  const out = [];
  walkDir(root, root, { include, exclude, skipDirs, excludeFile }, out);
  out.sort();
  return out;
}

function walkDir(root, dir, opts, out) {
  const entries = readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    if (entry.isDirectory()) {
      if (opts.skipDirs.has(entry.name)) continue;
      walkDir(root, path.join(dir, entry.name), opts, out);
      continue;
    }
    if (!entry.isFile()) continue; // symlinks/sockets/etc. — not a source file
    if (opts.excludeFile && opts.excludeFile.test(entry.name)) continue;
    const ext = path.extname(entry.name);
    if (opts.exclude.has(ext)) continue;
    if (opts.include && !opts.include.has(ext)) continue;
    const rel = path.relative(root, path.join(dir, entry.name)).split(path.sep).join('/');
    out.push(rel);
  }
}
