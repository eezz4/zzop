#!/usr/bin/env node
// Mode B adapter: the minimal "one channel" reference — fills ONLY dep-graph `imports` for a Java
// tree. Built against the v0.16-era lexical Java projector (`imports: None`, zero native dep
// edges); the native parser now extracts imports itself, so on today's Java trees this overlay is
// a no-op — kept as the teaching exhibit for the recipe: read each file's `package`/`import`
// declaration lines, project the intra-tree edges, nothing else.
// Rationale, contract points, and recall limits: README.md.
//
// USAGE:  node adapter.mjs --root <javaRoot> [--source <treeId>]  > overlay.json
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { walk, EnvelopeBuilder } from '../adapter-kit/index.js';

function arg(name, def) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : def;
}
const root = arg('root');
if (!root) {
  console.error('usage: node adapter.mjs --root <javaRoot> [--source <treeId>] > overlay.json');
  process.exit(2);
}

// `package a.b;` / `import [static] a.b.C;` / `import [static] a.b.C.*;` — line-anchored. A
// commented-out declaration inside a block comment can over-match (documented tolerance, README).
const PACKAGE_RE = /^\s*package\s+([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*)\s*;/m;
const IMPORT_RE = /^\s*import\s+(static\s+)?([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*(?:\.\*)?)\s*;/gm;

// Pass 1: index every fully-qualified class name in the tree from each file's OWN `package`
// declaration plus its filename — ground truth over directory-convention guessing, so a tree whose
// sources sit under any prefix (`src/main/java/`, `app/src/`, none) indexes correctly.
const rels = walk(root, { include: ['java'] });
const texts = new Map();
const classIndex = new Map(); // "com.example.util.TextUtil" -> "src/main/java/com/example/util/TextUtil.java"
for (const rel of rels) {
  const text = readFileSync(path.join(root, rel), 'utf8');
  texts.set(rel, text);
  const pkg = PACKAGE_RE.exec(text);
  const cls = path.posix.basename(rel, '.java');
  classIndex.set(pkg ? `${pkg[1]}.${cls}` : cls, rel);
}

// Pass 2: resolve each file's imports against the index. Only intra-tree edges are the goal — an
// import that names no file in the tree (JDK, external dependency) is SKIPPED, never guessed.
// `import a.b.*;` (package wildcard) is skipped too: knowing which classes it binds needs body
// analysis this adapter deliberately doesn't do. `import static a.b.C.member;` and
// `import static a.b.C.*;` both resolve to class C's file — the edge is to the class, whichever
// member is bound.
const builder = new EnvelopeBuilder({ parser: 'java-imports-adapter/1', source: arg('source', 'java') });
let fileCount = 0;
let edgeCount = 0;
let skipped = 0;
for (const rel of rels) {
  const imports = {};
  for (const m of texts.get(rel).matchAll(IMPORT_RE)) {
    const isStatic = Boolean(m[1]);
    let fq = m[2];
    if (fq.endsWith('.*')) {
      fq = fq.slice(0, -2);
      if (!isStatic) {
        skipped++; // package wildcard — documented skip
        continue;
      }
    } else if (isStatic) {
      fq = fq.slice(0, fq.lastIndexOf('.')); // static member import -> the owning class
    }
    const target = classIndex.get(fq);
    if (!target || target === rel) {
      skipped++; // JDK / external dependency / self — not an intra-tree edge
      continue;
    }
    // Specifier: RELATIVE to the importing file's dir, KEEPING the `.java` extension — the engine's
    // resolver tries the raw join first, so the exact target path resolves with no Java-specific
    // extension logic engine-side (see README "Contract points").
    let spec = path.posix.relative(path.posix.dirname(rel), target);
    if (!spec.startsWith('.')) spec = './' + spec;
    const last = m[2].split('.').pop();
    const local = last === '*' ? fq.split('.').pop() : last;
    const key = imports[local] ? `${local}$${Object.keys(imports).length}` : local;
    imports[key] = { specifier: spec, original: local };
    edgeCount++;
  }
  if (Object.keys(imports).length > 0) {
    builder.addFile(rel, { loc: texts.get(rel).split('\n').length, imports });
    fileCount++;
  }
}

process.stderr.write(
  `[java-imports-adapter] ${fileCount} projections, ${edgeCount} intra-tree import edges, ${skipped} skipped (external/wildcard/self)\n`
);
process.stdout.write(JSON.stringify(builder.toEnvelope()));
