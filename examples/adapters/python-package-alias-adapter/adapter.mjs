#!/usr/bin/env node
// python-package-alias-adapter — resolve `import <alias>.x.y` when the tree is INSTALLED under a name
// no file in it declares.
//
// The gap this fills is not a parser limitation. `twitter/the-algorithm-ml` imports itself as `tml`
// throughout, and nothing in the tree says so: `pyproject.toml` carries only `[tool.black]`, there is no
// `setup.py`/`setup.cfg`, and the real mechanism is one line of hand-written shell in
// `images/init_venv.sh`:
//
//     ln -s "$(pwd)" "$VENV_PATH/lib/python3.10/site-packages/tml"
//
// A symlink created at venv-build time, hardcoded to /opt/ee/python/3.10, Linux-only. No static reader
// can find that, and guessing "the repo root is probably importable under its own directory name" is
// the inference the engine refuses to make. So it is what adapter injection exists for: the user knows
// the alias, and telling zzop costs one flag.
//
// Usage:
//   node adapter.mjs --root <treeRoot> --alias tml [--source <treeId>] > overlay.json
//
// This adapter DECLARES OVERRIDES, and measuring is what proved it has to.
//
// The plan said additive merging would be enough: the native parser leaves a dead `tml.*` binding, the
// adapter supplies one that resolves, and both can coexist. That was wrong, and it was wrong in the way
// that is easy to miss — the two sides use the SAME LOCAL NAME. `from tml.common.filesystem import
// infer_fs` binds `infer_fs` natively (to the unresolvable specifier `tml.common.filesystem`) and this
// adapter binds `infer_fs` too (to `../filesystem`, which resolves). That is a collision, native wins by
// default, and the dead binding stays. Measured on `twitter/the-algorithm-ml`: 179 of 188 offered
// bindings were dropped that way, and the run said so — the drop disclosure is what diagnosed it.
//
// So the correcting binding must displace, not join. Each projection declares the local names whose
// native binding it replaces, which requires version >= 0.27.0 (see docs/NORMALIZED_AST.md). The engine
// reports every displacement with both specifiers; on this tree that is a long list, and it should be —
// each line is a native fact this adapter asserted was wrong.

import { readFileSync } from 'node:fs';
import path from 'node:path';
import { EnvelopeBuilder, walk } from '../adapter-kit/index.js';

function parseArgs(argv) {
  const args = { source: 'python-alias' };
  for (let i = 0; i < argv.length; i += 2) {
    const [flag, value] = [argv[i], argv[i + 1]];
    if (flag === '--root') args.root = value;
    else if (flag === '--alias') args.alias = value;
    else if (flag === '--source') args.source = value;
    else throw new Error(`unknown flag: ${flag}`);
  }
  if (!args.root || !args.alias) {
    throw new Error('usage: adapter.mjs --root <dir> --alias <packageName> [--source <id>]');
  }
  return args;
}

/** `tml.core.config` -> `core/config`; null when the dotted name is not under the alias. */
function aliasToRelative(alias, dotted) {
  if (dotted !== alias && !dotted.startsWith(`${alias}.`)) return null;
  const rest = dotted.slice(alias.length).replace(/^\./, '');
  return rest.length === 0 ? '' : rest.split('.').join('/');
}

/**
 * Every alias-rooted import in one file, as `{ localName, dotted, original }`.
 *
 * Line-based on purpose: this adapter resolves MODULE PATHS, and both statement forms put the whole
 * dotted name on their own line in real code. The local-name rules mirror CPython's, because they are
 * what the engine keys `imports` by — `import a.b.c` binds only `a`, `from a.b import c` binds `c`, and
 * `as` overrides both. Getting that wrong is not cosmetic: a binding under the wrong key arrives as a
 * SIBLING entry rather than the resolution the tree needs, which is the failure
 * `examples/adapters/override-required` was built to demonstrate.
 */
function extractAliasImports(text, alias) {
  const out = [];
  for (const raw of text.split('\n')) {
    const line = raw.trim();

    const from = /^from\s+([\w.]+)\s+import\s+(.+)$/.exec(line);
    if (from) {
      const dotted = from[1];
      if (aliasToRelative(alias, dotted) === null) continue;
      for (const piece of from[2].replace(/[()]/g, '').split(',')) {
        const name = piece.trim();
        if (!name || name === '*') continue; // a star import binds no single name we can key
        const parts = name.split(/\s+as\s+/);
        out.push({ localName: parts[1] ?? parts[0], dotted, original: parts[0] });
      }
      continue;
    }

    const plain = /^import\s+([\w.,\s]+)$/.exec(line);
    if (plain) {
      for (const piece of plain[1].split(',')) {
        const name = piece.trim();
        if (!name) continue;
        const parts = name.split(/\s+as\s+/);
        const dotted = parts[0];
        if (aliasToRelative(alias, dotted) === null) continue;
        // No `as`: Python binds only the top-level package segment.
        out.push({ localName: parts[1] ?? dotted.split('.')[0], dotted, original: '*' });
      }
    }
  }
  return out;
}

const args = parseArgs(process.argv.slice(2));
const builder = new EnvelopeBuilder({
  parser: 'python-package-alias-adapter/1',
  source: args.source,
});

for (const rel of walk(args.root, { include: ['.py'] })) {
  const text = readFileSync(path.join(args.root, rel), 'utf8');
  const found = extractAliasImports(text, args.alias);
  if (found.length === 0) continue; // project only files that carry a fact

  const imports = {};
  for (const { localName, dotted, original } of found) {
    const target = aliasToRelative(args.alias, dotted);
    // Emit a path RELATIVE TO THE IMPORTING FILE and let the engine's own resolver try `<x>.py` and
    // `<x>/__init__.py` against the files that actually exist. A candidate is a question, not a claim:
    // a name that resolves to neither simply stays unresolved, exactly as it does without this overlay.
    const fromDir = path.posix.dirname(rel);
    let specifier = path.posix.relative(fromDir === '.' ? '' : fromDir, target);
    if (!specifier.startsWith('.')) specifier = `./${specifier}`;
    imports[localName] = { specifier, original };
  }
  // Every name here is one the native parser also bound, to a specifier that resolves to nothing —
  // `import tml.x` is unresolvable precisely because no file in the tree declares the alias. Declaring
  // the displacement is what makes the correction take effect instead of losing its own collision.
  builder.addFile(rel, {
    loc: text.split('\n').length,
    imports,
    overrides: { imports: Object.keys(imports) },
  });
}

process.stdout.write(`${JSON.stringify(builder.toEnvelope())}\n`);
