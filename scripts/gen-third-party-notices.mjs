/* Generate (and verify) THIRD-PARTY-NOTICES.md from `cargo metadata`.
 *
 * WHY THIS EXISTS
 * zzop ships as a STATICALLY LINKED binary. Linking does not dissolve the upstream licenses it
 * links: Apache-2.0 §4(a) requires handing every recipient a copy of the license, and `smartstring`
 * is MPL-2.0+, whose §3.2 requires telling recipients how to obtain the source. Our own MIT notice
 * discharges none of that. Until this file existed (2026-07-29) the repository shipped 170 statically
 * linked third-party crates and not one line of notice for any of them.
 *
 * WHY NOT cargo-about / cargo-deny
 * They are the obvious answer and they are a heavyweight dev dependency plus a config file plus a
 * template, for a workspace whose entire need is "list the deps and paste the license texts". This
 * file is ~400 lines of Node with no dependencies, and it reads the same `cargo metadata` those tools
 * read. If the notices ever need SPDX expression EVALUATION (choosing one arm of `MIT OR Apache-2.0`
 * rather than reproducing both), revisit — that is the point where a real SPDX library earns its
 * keep. Today we reproduce EVERY arm, which is always safe and never requires evaluating anything.
 *
 * NOTHING HERE IS HAND-LISTED
 * The dependency set is a graph walk, the license identifiers are parsed out of the SPDX expressions
 * the crates themselves declare, and the license bodies are read from the crates' own files in the
 * cargo registry. There is no array of crate names in this file, and there must never be one: a
 * hand-listed inventory is correct the day it is written and silently wrong from the next `cargo
 * update` onward, which is precisely the class this repository keeps paying for.
 *
 * THE DEPENDENCY SET IS DELIBERATELY A SUPERSET
 * `cargo metadata` is invoked WITHOUT --filter-platform, so target-specific dependencies for every
 * platform are included. The npm lane publishes five platform packages from one source tree; a
 * host-filtered walk would produce notices that are correct on the machine that ran this script and
 * under-report on the other four. Over-reporting a crate we do not link on some platform costs a
 * table row. Under-reporting one we do link is a license violation.
 *
 * WHAT COUNTS AS A DEPENDENCY
 * Reachable from a workspace member through NORMAL edges only. `dev` edges (test/bench-only) and
 * `build` edges (build scripts, which do not end up in the shipped artifact) are excluded — those
 * crates are not distributed, so no notice duty attaches to them.
 *
 * SILENT OMISSION IS THE WORST FAILURE MODE
 * Every place a license text could not be found says so, loudly, IN THE GENERATED FILE — by crate in
 * the "publish no license text" section, and by identifier as a NO LOCAL LICENSE TEXT FOUND block
 * where the text would have been. A notices file that quietly drops the one license it could not
 * resolve is worse than no notices file: it looks like diligence.
 *
 * USAGE
 *   node scripts/gen-third-party-notices.mjs            # regenerate THIRD-PARTY-NOTICES.md
 *   node scripts/gen-third-party-notices.mjs --check    # verify it, exit 1 on drift (no writes)
 *
 * GENERATION NEEDS cargo. VERIFICATION DOES NOT.
 * `--check` runs entirely offline: no `cargo`, no registry index, no network, no toolchain. That is
 * deliberate and it is the reason the fingerprints below exist. The guard that calls --check
 * (scripts/check-license-shipping.sh) runs in a CI job that checks out the repo and installs
 * nothing, and in a pre-commit hook that is already 4m25s; making it fetch a sparse registry index
 * (and possibly a rust-std for a target the runner lacks) would give a licensing guard a brand-new
 * way to go red for reasons that have nothing to do with licensing.
 *
 * HOW --check STAYS EXACT WITHOUT cargo
 * The shipped inventory is a pure function of two committed inputs: Cargo.lock and the workspace's
 * Cargo.toml manifests. If neither has changed since this file was generated, the inventory CANNOT
 * have changed, so the committed table is provably current. Generation therefore records
 *   fingerprint inventory-inputs -- a digest of every tracked Cargo.lock / Cargo.toml, enumerated by
 *     `git ls-files` (DERIVED, never a hand-written path list: a guard that iterates a list written
 *     inside its own file cannot see the set grow, and this repo has already paid for that twice).
 *   fingerprint generated-body -- a digest of this file itself with the two fingerprint lines
 *     removed, recorded at write time.
 * and --check recomputes both. A mismatch is a FAILURE that says "regenerate" -- never a silent
 * skip, never an automatic rewrite. The inventory is NOT re-derived from Cargo.lock directly, and
 * must not be: the lockfile does not distinguish normal from dev/build edges, so a lock-derived set
 * would be a superset of what actually ships and would never match this table.
 *
 * WHAT EACH FINGERPRINT DOES AND DOES NOT COVER
 * inventory-inputs proves the INVENTORY is current. It proves nothing whatsoever about this file's
 * own bytes: a rewritten table row, a deleted or gutted `### <id>` section, an invented crate --
 * none of those touch a manifest, so that digest cannot see any of them. generated-body closes
 * exactly that direction. On top of it --check keeps the semantic checks (Summary counts against the
 * parsed table, the table's own SPDX identifiers against the `### ` sections, every section carrying
 * a non-trivial body or an explicit unavailable marker, the textless list being a subset of the
 * table), so the common hand edits fail with a message that names what is wrong rather than only
 * "these bytes moved".
 *
 * --check still does NOT re-read the upstream license BODIES: those live in the cargo registry's
 * extracted `src/` directory, which exists only on a machine that has actually built the workspace.
 * Requiring them would make the guard fail on a fresh checkout for reasons unrelated to compliance.
 * Regeneration reads them; verification does not. This limitation is stated here, in
 * scripts/check-license-shipping.sh, and nowhere else is it implied to be otherwise.
 *
 * DETERMINISM
 * Sorted everywhere, no timestamps, no host paths in the output. Run it twice on the same lockfile
 * and registry and the bytes are identical. Line endings follow the committed file's own convention
 * (this repo checks out CRLF on Windows, LF on Linux; git normalizes to LF in the index), and
 * --check compares EOL-normalized so neither platform can produce a spurious failure.
 */
import fs from 'fs';
import path from 'path';
import { createHash } from 'crypto';
import { spawnSync } from 'child_process';
import { fileURLToPath } from 'url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const OUTPUT = path.join(REPO_ROOT, 'THIRD-PARTY-NOTICES.md');
const SELF = 'gen-third-party-notices';

/* A license body shorter than this is not a license. Shared by the writer (which downgrades such a
 * find to an explicit "unavailable" block rather than pasting a stub) and by --check (which asserts
 * every section clears the same bar), so the two can never disagree about what counts as present. */
const MIN_TEXT_CHARS = 200;
const UNAVAILABLE_MARKER = 'NO LOCAL LICENSE TEXT FOUND';

/* The committed inputs the shipped inventory is a pure function of, as `git ls-files` pathspecs.
 * fnmatch WITHOUT FNM_PATHNAME, so a star crosses a slash and the second glob below reaches every
 * depth (crates/…, parser/…, rules/native/…). This is the ONE place the input set is named, and it is
 * named as a pattern, never as a list of paths -- a new crate joins the fingerprint by existing. */
const INVENTORY_INPUT_GLOBS = ['Cargo.lock', 'Cargo.toml', '*/Cargo.toml'];

/* The two recorded digests, and the exact shape of the lines that carry them. Both the digest
 * computation (which must exclude these lines, or the body digest would depend on itself) and the
 * reader parse through this one regex, so they cannot disagree about which lines are fingerprints. */
const FINGERPRINT_NAMES = ['inventory-inputs', 'generated-body'];
const FINGERPRINT_LINE_RE = /^     fingerprint (inventory-inputs|generated-body) = (.*)$/;
const fingerprintLine = (name, value) => `     fingerprint ${name} = ${value}`;
const FINGERPRINT_PENDING = '<computed at write time>';

function die(msg) {
  process.stderr.write(`${SELF}: ${msg}\n`);
  process.exit(1);
}

/* ---------------------------------------------------------------------------------------------- */
/* Fingerprints (the offline half -- git only, no cargo, no network)                                 */
/* ---------------------------------------------------------------------------------------------- */

function git(args, why) {
  const r = spawnSync('git', args, { cwd: REPO_ROOT, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  if (r.error) die(`\`git ${args.join(' ')}\` could not be spawned: ${r.error.message}.\n  ${why}`);
  if (r.status !== 0) die(`\`git ${args.join(' ')}\` exited ${r.status}:\n${(r.stderr || '').trim()}\n  ${why}`);
  return r.stdout;
}

/* Digest of a rendered/committed file, with the fingerprint lines themselves removed. Callers pass
 * EOL-NORMALIZED content on both sides (write joins with '\n', check normalizes what it read), so a
 * CRLF checkout on Windows and an LF checkout on Linux produce the same digest for the same file. */
function bodyDigest(normalizedContent) {
  const kept = normalizedContent.split('\n').filter((l) => !FINGERPRINT_LINE_RE.test(l));
  return createHash('sha256').update(kept.join('\n'), 'utf8').digest('hex');
}

/* Digest of every tracked Cargo.lock / Cargo.toml, as a single pair of git processes.
 *
 * `git hash-object` WITHOUT --no-filters is the deliberate spelling: it hashes the working tree
 * (which is the bytes this repo's guards judge -- see .githooks/pre-commit) THROUGH the clean
 * filter, i.e. exactly the blob `git add` would store. That makes the digest identical on a CRLF
 * Windows checkout and an LF Linux one; --no-filters would make every fingerprint cross-platform
 * garbage. Verified locally: the filtered hash of Cargo.toml equals its index blob hash.
 *
 * SUBJECT-SET FLOOR: an empty or lockless enumeration aborts. A digest over zero files is a
 * constant, and a constant fingerprint matches forever -- the vacuous green this repo has paid for
 * before. Neither zero is possible in this repository; either one means the pathspec broke. */
function inventoryFingerprint() {
  const why = 'The fingerprint that makes --check exact without cargo is derived from git; without git there is no verification to report.';
  const files = git(['ls-files', '-z', '--', ...INVENTORY_INPUT_GLOBS], why).split('\0').filter((p) => p.length > 0);
  files.sort();

  if (files.length === 0) {
    die(
      `\`git ls-files -- ${INVENTORY_INPUT_GLOBS.join(' ')}\` matched NOTHING, so the inventory\n` +
        '  fingerprint would be a digest over zero files -- a constant that matches forever. This is a\n' +
        '  Rust workspace with a committed lockfile; a zero here is a broken enumeration, never an\n' +
        '  empty input set.'
    );
  }
  if (!files.includes('Cargo.lock')) {
    die(
      'Cargo.lock is not tracked, so the resolved dependency versions are not an input this\n' +
        '  fingerprint can see. Commit the lockfile; a binary-shipping workspace must have one.'
    );
  }
  const manifests = files.filter((p) => p === 'Cargo.toml' || p.endsWith('/Cargo.toml'));
  if (manifests.length === 0) {
    die('the enumeration found a Cargo.lock but ZERO Cargo.toml manifests. The pathspec broke.');
  }

  const hashes = git(['hash-object', '--', ...files], why)
    .split('\n')
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  if (hashes.length !== files.length) {
    die(
      `git hash-object returned ${hashes.length} hash(es) for ${files.length} file(s). Refusing to\n` +
        '  fingerprint an input set that was not fully hashed -- the missing files would be invisible\n' +
        '  to every future --check.'
    );
  }

  const h = createHash('sha256');
  for (let i = 0; i < files.length; i++) h.update(`${hashes[i]} ${files[i]}\n`);
  return { digest: h.digest('hex'), count: files.length };
}

/* ---------------------------------------------------------------------------------------------- */
/* cargo metadata                                                                                   */
/* ---------------------------------------------------------------------------------------------- */

/* --offline first, because the registry cache is populated on any machine that has built this
 * workspace and offline is both faster and immune to a registry outage. The retry without --offline
 * is not a fallback to sloppiness: it is what makes this runnable in CI, where the checkout has a
 * Cargo.lock but no populated registry, and it resolves the SAME locked versions. If both fail we
 * abort with both stderrs — never with a partial graph, which would silently drop notice duties. */
function cargoMetadata() {
  const attempts = [
    ['metadata', '--format-version', '1', '--offline'],
    ['metadata', '--format-version', '1'],
  ];
  const errors = [];
  for (const args of attempts) {
    const r = spawnSync('cargo', args, {
      cwd: REPO_ROOT,
      encoding: 'utf8',
      maxBuffer: 256 * 1024 * 1024,
      shell: process.platform === 'win32',
    });
    if (r.error) {
      errors.push(`\`cargo ${args.join(' ')}\` could not be spawned: ${r.error.message}`);
      continue;
    }
    if (r.status !== 0) {
      errors.push(`\`cargo ${args.join(' ')}\` exited ${r.status}:\n${(r.stderr || '').trim()}`);
      continue;
    }
    return JSON.parse(r.stdout);
  }
  /* Name the ACTUAL cause when the tool is simply not installed. The generic "could not obtain
   * cargo metadata" text below, printed at a missing-toolchain machine, sends the reader hunting a
   * dependency-graph problem that does not exist. One extra spawn, only on the failure path. */
  const probe = spawnSync('cargo', ['--version'], {
    cwd: REPO_ROOT,
    encoding: 'utf8',
    shell: process.platform === 'win32',
  });
  if (probe.error || probe.status !== 0) {
    die(
      '`cargo` was not found on PATH, so the dependency inventory cannot be derived.\n' +
        '  GENERATING THIRD-PARTY-NOTICES.md requires a cargo toolchain: it walks `cargo metadata` for\n' +
        '  the shipped dependency graph and reads license texts out of the cargo registry. VERIFYING it\n' +
        '  does not -- `node scripts/gen-third-party-notices.mjs --check` is fully offline and needs\n' +
        '  only git. Install a toolchain to regenerate, or use --check if you meant to verify.'
    );
  }
  die(
    'could not obtain cargo metadata. This file is DERIVED; producing it from a partial or absent\n' +
      '  dependency graph would silently under-report notice duties, so this aborts instead.\n  ' +
      errors.join('\n  ')
  );
}

/* Packages reachable from a workspace member through normal (non-dev, non-build) edges, excluding
 * the workspace's own crates. */
function shippedDependencies(meta) {
  const byId = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const workspace = new Set(meta.workspace_members);

  const seen = new Set();
  const stack = [...workspace];
  while (stack.length > 0) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    const node = nodes.get(id);
    if (!node) continue;
    for (const dep of node.deps) {
      /* dep_kinds is a newer field; an empty array from an older cargo means "kind unknown". Treat
       * unknown as normal — over-reporting a dev dependency costs a row, dropping a linked one is a
       * violation. */
      const kinds = dep.dep_kinds || [];
      const normal = kinds.length === 0 || kinds.some((k) => k.kind === null || k.kind === 'normal');
      if (normal) stack.push(dep.pkg);
    }
  }

  const out = [];
  for (const id of seen) {
    if (workspace.has(id)) continue;
    const p = byId.get(id);
    if (!p) die(`resolve graph references package id ${id} that is absent from metadata.packages`);
    out.push(p);
  }
  out.sort(cmpPkg);
  return out;
}

function cmpPkg(a, b) {
  if (a.name !== b.name) return a.name < b.name ? -1 : 1;
  if (a.version !== b.version) return a.version < b.version ? -1 : 1;
  return 0;
}

/* ---------------------------------------------------------------------------------------------- */
/* SPDX expressions                                                                                 */
/* ---------------------------------------------------------------------------------------------- */

/* Split an SPDX expression into the identifiers it names. `WITH` binds tighter than OR/AND and is
 * kept attached (`Apache-2.0 WITH LLVM-exception` is its own identifier with its own text). The
 * legacy `A/B` slash spelling — still used by four crates here — means the same as `A OR B`.
 * Every arm of an OR is reported, deliberately: choosing an arm is a legal decision, not a
 * scripting one, and reproducing all of them is unconditionally safe. */
function licenseIds(expr) {
  if (!expr) return [];
  return expr
    .replace(/[()]/g, ' ')
    .split(/\s+(?:OR|AND)\s+|\//g)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

/* Filename hints per identifier. Crates that carry several license files name them
 * (LICENSE-MIT / LICENSE-APACHE / LICENSE-UNICODE / ...), and picking the wrong one would paste the
 * MIT text under an Apache heading — a confident, wrong notice. `deny` exists for Apache-2.0, whose
 * plain hint would otherwise also match LICENSE-Apache-2.0_WITH_LLVM-exception. */
const FILENAME_HINTS = {
  MIT: { allow: /mit/i },
  'Apache-2.0': { allow: /apache/i, deny: /llvm/i },
  'Apache-2.0 WITH LLVM-exception': { allow: /llvm/i },
  'BSD-2-Clause': { allow: /bsd/i },
  'BSD-3-Clause': { allow: /bsd/i },
  'BSL-1.0': { allow: /boost|bsl/i },
  Zlib: { allow: /zlib/i },
  Unlicense: { allow: /unlicen[cs]e/i },
  'Unicode-3.0': { allow: /unicode/i },
  'Unicode-DFS-2016': { allow: /unicode/i },
  'MPL-2.0': { allow: /mpl|mozilla/i },
  'MPL-2.0+': { allow: /mpl|mozilla/i },
};

const LICENSE_FILE_RE = /^(licen[cs]e|copying|unlicense)/i;
const GENERIC_LICENSE_FILE_RE = /^(licen[cs]e|copying|unlicense)(\.(md|txt))?$/i;

/* Basename of a relative path from licenseFilesOf(). Root-level files are bare names; files found
 * one directory down are `subdir/name`. Hints match the BASENAME (a directory named, say, `limits/`
 * must not satisfy the MIT hint), while GENERIC_LICENSE_FILE_RE stays anchored on the full relative
 * path — a bare LICENSE in a subdirectory is exactly the vendored-third-party shape whose license
 * may not be the crate's own, so the generic pass never descends. */
const baseName = (rel) => rel.slice(rel.lastIndexOf('/') + 1);

/* The two predicates `derive()` picks WITH. They live here, named and exported, rather than inline
 * at their call sites, because scripts/test-notices-harvest.mjs must drive THESE — a fixture that
 * reconstructs them locally tests its own copy, which is what it did until 2026-08-01: mutating
 * either call site left the fixture green while it printed a line claiming both were covered.
 *
 * `hintPredicate` matches on the BASENAME so a directory named after a license cannot satisfy a
 * hint meant for a file. `genericPredicate` matches on the FULL relative path, which is what keeps
 * the generic pass from descending — a subdirectory candidate always carries a `/`. Neither is a
 * style choice; both are the behavior a mutation test pins. */
const hintPredicate = (hint) => (name) => {
  const base = baseName(name);
  return hint.allow.test(base) && !(hint.deny && hint.deny.test(base));
};
const genericPredicate = (name) => GENERIC_LICENSE_FILE_RE.test(name);

/* Subdirectories the one-level descent must NOT enter. The hazard: a crate that vendors another
 * project ships that project's OWN license under vendor/ or third_party/ (a vendored LICENSE-MIT,
 * say), and the descent would hand it to the hint pass as a candidate for the CRATE's own license —
 * a confident, wrong notice reproducing somebody else's license text under this crate's heading.
 * The witnessed legitimate case stays covered: unicode_names2-1.3.0/data/LICENSE-UNICODE lives
 * under data/, which is not in this list. */
const SKIPPED_SUBDIRS = new Set(['vendor', 'vendored', 'third party', 'deps', 'testdata', 'tests']);

/* Membership is tested on a NORMALIZED name: lowercased, with `-` and `_` both folded to a space.
 * The list held only `third_party` until 2026-08-01, so `third-party/` — at least as common in
 * published crates — walked straight past it and its vendored LICENSE-MIT reached the hint pass.
 * Folding the separator means a future entry cannot be defeated by the other spelling. */
const skippedSubdir = (name) => SKIPPED_SUBDIRS.has(name.toLowerCase().replace(/[-_]/g, ' '));

/* A SUBDIRECTORY candidate must additionally not look like SOURCE. The thing being excluded is
 * src/license_mit.rs, which qualifies by name prefix and whose basename satisfies the MIT hint.
 *
 * This was an ALLOW-list (extensionless | .md | .txt) until 2026-08-01, and it silently dropped the
 * spellings the comment beside it already knew about: LICENSE-APACHE-2.0 and LICENSE-BSD-3.0 read
 * their VERSION as a suffix, and COPYING.LESSER / LICENSE.rst are ordinary license-text names. All
 * four were measured lost. The failure is invisible — an identifier whose only carrier keeps its
 * text at licenses/LICENSE-APACHE-2.0 degrades to NO LOCAL LICENSE TEXT FOUND, and check() steps
 * past that marker rather than failing on it.
 *
 * A deny-list is the correct shape here because the population being excluded is finite and known
 * (source and binary files) while license spellings are open. Root candidates stay unconstrained. */
const SOURCE_LIKE_EXT =
  /\.(rs|js|mjs|cjs|ts|tsx|jsx|py|go|java|cs|c|h|cc|cpp|hpp|rb|php|sh|bat|ps1|toml|json|yaml|yml|lock|so|dll|dylib|a|o|exe|wasm|png|jpe?g|gif|svg|ico|zip|gz|tar)$/i;
const plausibleSubdirLicenseFile = (name) => !SOURCE_LIKE_EXT.test(name);

/* License-named files in the crate root, plus ONE level of subdirectories. The descent exists
 * because published packages really do ship a license only there: unicode_names2 1.3.0 carries its
 * Unicode-DFS-2016 text solely at data/LICENSE-UNICODE (found 2026-07-31, after this file shipped a
 * NO LOCAL LICENSE TEXT FOUND block for an identifier whose text was sitting one directory down).
 * Root files sort first, so WITHIN ONE CRATE a root-level match beats a subdirectory one — only
 * within one crate: pick() is crate-major, so a text found in ANY earlier-ranked crate (root or
 * subdirectory) still wins over a later crate's root file. */
function licenseFilesOf(pkg) {
  const dir = path.dirname(pkg.manifest_path);
  let entries;
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return { dir, files: [], readable: false };
  }
  const files = entries
    .filter((e) => e.isFile() && LICENSE_FILE_RE.test(e.name))
    .map((e) => e.name)
    .sort();
  const subFiles = [];
  for (const e of entries) {
    if (!e.isDirectory()) continue;
    if (skippedSubdir(e.name.toLowerCase())) continue; /* vendored code's license is not the crate's */
    let sub;
    try {
      sub = fs.readdirSync(path.join(dir, e.name), { withFileTypes: true });
    } catch {
      continue;
    }
    for (const s of sub) {
      if (s.isFile() && LICENSE_FILE_RE.test(s.name) && plausibleSubdirLicenseFile(s.name)) {
        subFiles.push(`${e.name}/${s.name}`);
      }
    }
  }
  subFiles.sort();
  return { dir, files: [...files, ...subFiles], readable: true };
}

function readText(dir, name) {
  try {
    return fs.readFileSync(path.join(dir, name), 'utf8').replace(/\r\n/g, '\n').trimEnd();
  } catch {
    return null;
  }
}

/* ---------------------------------------------------------------------------------------------- */
/* Derivation                                                                                       */
/* ---------------------------------------------------------------------------------------------- */

/* Always reads the license bodies: the only caller left is write(). --check used to call this with
 * readBodies=false to re-derive the inventory from cargo metadata; it no longer derives anything
 * from cargo at all, so that branch is gone rather than left as an unreachable second path. */
function derive() {
  const meta = cargoMetadata();
  const deps = shippedDependencies(meta);
  if (deps.length === 0) {
    die(
      'the dependency walk produced ZERO third-party crates. This workspace links tree-sitter, swc\n' +
        '  and ruff among others; a zero means the graph walk broke, never that we vendored nothing.\n' +
        '  Refusing to write an empty notices file, which would read as "no obligations".'
    );
  }

  const undeclared = deps.filter((p) => !p.license && !p.license_file);
  if (undeclared.length > 0) {
    die(
      'these dependencies declare no license at all in their published metadata, so no notice can\n' +
        '  be derived for them. Resolve them by hand before regenerating:\n  ' +
        undeclared.map((p) => `${p.name} ${p.version}`).join('\n  ')
    );
  }

  /* identifier -> crates that carry it */
  const idToCrates = new Map();
  for (const p of deps) {
    for (const id of licenseIds(p.license)) {
      if (!idToCrates.has(id)) idToCrates.set(id, []);
      idToCrates.get(id).push(p);
    }
  }
  const ids = [...idToCrates.keys()].sort();

  /* Crates whose published package carries no license file of its own. Reported by name: these are
   * exactly the crates whose Apache-2.0 §4(a) / MIT notice duty we discharge from a SIBLING crate's
   * copy of the same license, and a reader deserves to know which. */
  const textless = [];
  const perCrateFiles = new Map();
  for (const p of deps) {
    const info = licenseFilesOf(p);
    perCrateFiles.set(p.id, info);
    if (info.files.length === 0) textless.push(p);
  }

  /* Two passes so a hint match from ANY crate beats a generic filename from an earlier one. */
  const texts = new Map();
  for (const id of ids) {
    const hint = FILENAME_HINTS[id];
    const crates = idToCrates.get(id);
    let chosen = null;

    if (hint) {
      chosen = pick(crates, perCrateFiles, hintPredicate(hint));
    }
    if (!chosen) {
      /* A crate licensed under exactly one identifier can only mean THAT identifier by its
       * unqualified LICENSE/COPYING file. A multi-license crate's bare LICENSE is ambiguous and is
       * deliberately never used here. */
      const single = crates.filter((p) => licenseIds(p.license).length === 1);
      chosen = pick(single, perCrateFiles, genericPredicate);
    }

    if (chosen && chosen.text.length < MIN_TEXT_CHARS) {
      chosen = {
        ...chosen,
        text: null,
        reason:
          `the only local file found was ${chosen.source} (${chosen.text.length} bytes), which is too ` +
          `short to be a license text`,
      };
    }
    texts.set(
      id,
      chosen || {
        text: null,
        source: null,
        reason:
          'no crate carrying this identifier ships a matching LICENSE/COPYING file in its published ' +
          'package, so no verbatim text could be sourced locally',
      }
    );
  }

  return { deps, ids, idToCrates, texts, textless };
}

function pick(crates, perCrateFiles, nameMatches) {
  for (const p of crates) {
    const info = perCrateFiles.get(p.id);
    if (!info) continue;
    for (const name of info.files) {
      if (!nameMatches(name)) continue;
      const text = readText(info.dir, name);
      if (text === null) continue;
      return { text, source: `${p.name}-${p.version}/${name}`, crate: p };
    }
  }
  return null;
}

/* ---------------------------------------------------------------------------------------------- */
/* Rendering                                                                                        */
/* ---------------------------------------------------------------------------------------------- */

/* A license body containing a ``` line would break out of its own fence. Compute the fence rather
 * than assume; the alternative is a file that renders correctly today and corrupts the moment some
 * upstream ships a fenced example inside its license. */
function fenceFor(text) {
  let longest = 0;
  for (const m of text.matchAll(/^`{3,}/gm)) longest = Math.max(longest, m[0].length);
  return '`'.repeat(Math.max(3, longest + 1));
}

function mdCell(s) {
  return String(s).replace(/\|/g, '\\|');
}

function render({ deps, ids, idToCrates, texts, textless }) {
  const L = [];
  L.push('# Third-Party Notices');
  L.push('');
  L.push('<!-- GENERATED FILE - DO NOT EDIT BY HAND.');
  L.push('     Regenerate: node scripts/gen-third-party-notices.mjs');
  L.push('     Enforced by: scripts/check-license-shipping.sh');
  L.push('');
  L.push('     The two digests below let that guard verify this file EXACTLY and OFFLINE: no cargo, no');
  L.push('     registry, no network. A mismatch is a FAILURE telling the author to regenerate. It is');
  L.push('     never skipped and never repaired automatically.');
  L.push('');
  L.push('     inventory-inputs digests every tracked Cargo.lock and Cargo.toml, enumerated by');
  L.push('     `git ls-files` and never by a hand-written path list. The shipped inventory is a pure');
  L.push('     function of those files, so an unchanged digest proves the table below is still current.');
  L.push('');
  L.push('     WHAT inventory-inputs DOES NOT COVER: the bytes of this file. No hand edit here - a');
  L.push('     rewritten table row, a deleted or gutted `### <id>` section, an invented crate - touches');
  L.push('     a manifest, so that digest cannot see any of them. generated-body closes exactly that');
  L.push('     direction: it is the digest of this file with the two `fingerprint` lines removed, taken');
  L.push('     when it was written. The semantic checks run on top of it (Summary counts against the');
  L.push('     table, the table\'s SPDX identifiers against the `### ` sections, every section carrying a');
  L.push('     real body or an explicit "NO LOCAL LICENSE TEXT FOUND" marker, the textless list being a');
  L.push('     subset of the table) so the common hand edits fail by name, not merely as moved bytes.');
  L.push('');
  for (const name of FINGERPRINT_NAMES) L.push(fingerprintLine(name, FINGERPRINT_PENDING));
  L.push('-->');
  L.push('');
  L.push('The `zzop` binaries are statically linked. The crates listed here are compiled into every');
  L.push('artifact this project distributes (GitHub release binaries, the `@zzop/cli` npm packages, and');
  L.push('the `.mcpb` bundle), so their licenses travel with those artifacts. zzop itself is MIT; see the');
  L.push('`LICENSE` file that ships beside this one.');
  L.push('');
  L.push('This file is generated from `cargo metadata`. The dependency set is every crate reachable from');
  L.push('a workspace member through normal (non-dev, non-build) edges, for **all** target platforms -');
  L.push('deliberately a superset, because one source tree is published for five platforms.');
  L.push('');
  L.push('## Summary');
  L.push('');
  L.push(`- Third-party crates linked: **${deps.length}**`);
  L.push(`- Distinct license identifiers: **${ids.length}**`);
  L.push(`- Crates whose published package carries no license text of its own: **${textless.length}**`);
  L.push('');
  L.push('## Dependencies');
  L.push('');
  L.push('| Crate | Version | License (SPDX) | Repository |');
  L.push('| --- | --- | --- | --- |');
  for (const p of deps) {
    const repo = p.repository ? mdCell(p.repository) : '_none declared_';
    L.push(`| ${mdCell(p.name)} | ${mdCell(p.version)} | ${mdCell(p.license)} | ${repo} |`);
  }
  L.push('');

  L.push('## Crates that publish no license text of their own');
  L.push('');
  if (textless.length === 0) {
    L.push('None - every crate above ships its own license file.');
  } else {
    L.push('These crates declare a license in their metadata but ship no `LICENSE`/`COPYING` file inside');
    L.push('their published package, so the verbatim text reproduced below for their identifier is taken');
    L.push('from another crate under the same license. This is recorded rather than hidden: the reader is');
    L.push('entitled to know which notices are sourced second-hand.');
    L.push('');
    L.push('| Crate | Version | License (SPDX) |');
    L.push('| --- | --- | --- |');
    for (const p of [...textless].sort(cmpPkg)) {
      L.push(`| ${mdCell(p.name)} | ${mdCell(p.version)} | ${mdCell(p.license)} |`);
    }
  }
  L.push('');

  L.push('## License texts');
  L.push('');
  for (const id of ids) {
    const crates = idToCrates.get(id);
    const entry = texts.get(id);
    L.push(`### ${id}`);
    L.push('');
    L.push(`Applies to ${crates.length} crate(s): ${crates.map((p) => `\`${p.name}\``).join(', ')}`);
    L.push('');
    if (entry.text) {
      L.push(`Reproduced verbatim from \`${entry.source}\`.`);
      L.push('');
      const fence = fenceFor(entry.text);
      L.push(fence);
      L.push(...entry.text.split('\n'));
      L.push(fence);
    } else {
      L.push(`> **${UNAVAILABLE_MARKER}** for \`${id}\`: ${entry.reason}.`);
      L.push('>');
      L.push('> The obligation is not waived by this. Obtain the text from the crates listed above (their');
      L.push('> repositories are in the dependency table) or from https://spdx.org/licenses/ and add it');
      L.push('> before distributing. This block is emitted rather than the section being omitted, because');
      L.push('> a silently missing license is indistinguishable from a license that was never owed.');
    }
    L.push('');
  }

  return L;
}

/* ---------------------------------------------------------------------------------------------- */
/* Write / check                                                                                    */
/* ---------------------------------------------------------------------------------------------- */

function normalize(s) {
  return s.replace(/\r\n/g, '\n');
}

function detectEol() {
  if (fs.existsSync(OUTPUT) && fs.readFileSync(OUTPUT, 'utf8').includes('\r\n')) return '\r\n';
  if (fs.existsSync(OUTPUT)) return '\n';
  /* First creation: match what this platform's checkout convention would produce. git normalizes to
   * LF in the index either way (core.autocrlf), and --check is EOL-insensitive, so this choice is
   * cosmetic and cannot cause a cross-platform failure. */
  return process.platform === 'win32' ? '\r\n' : '\n';
}

function write() {
  /* The inventory fingerprint is taken BEFORE the cargo walk, so a manifest edited mid-run cannot
   * be recorded as the input of a graph that predates it. */
  const inventory = inventoryFingerprint();
  const lines = render(derive());

  /* bodyDigest() drops every fingerprint line, so it is computed over the same bytes --check will
   * reconstruct, and is independent of the placeholder values it replaces. */
  const values = {
    'inventory-inputs': inventory.digest,
    'generated-body': bodyDigest(lines.join('\n') + '\n'),
  };
  const stamped = lines.map((l) => {
    const m = l.match(FINGERPRINT_LINE_RE);
    return m ? fingerprintLine(m[1], values[m[1]]) : l;
  });

  const eol = detectEol();
  fs.writeFileSync(OUTPUT, stamped.join(eol) + eol, 'utf8');
  process.stdout.write(
    `${SELF}: wrote ${path.relative(REPO_ROOT, OUTPUT)} (${stamped.length} lines; ` +
      `inventory fingerprint over ${inventory.count} tracked manifest file(s)).\n`
  );
}

/* Parse the committed file back into the two facts --check compares: the dependency table, and the
 * per-identifier license-text sections. Parsing our own output is intentional — the alternative is
 * regenerating in full and diffing, which would demand the extracted cargo registry that CI does not
 * have (see this file's header).
 *
 * MUST BE FENCE-AWARE, and was not on its first draft. The MPL-2.0 text is itself Markdown: it has
 * `### 1. Definitions` ... `### 10. Versions of the License` and `## Exhibit A` headings inside it.
 * A line-prefix parser read those as ten new license-identifier sections and as a section-boundary
 * that ended the "License texts" section entirely, so --check reported twenty-four fictional
 * problems against a file that was in fact perfectly in sync. A license text can contain literally
 * any Markdown, so nothing inside a fence may be interpreted at all — only the fence's own
 * delimiters. Closing fences must be at least as long as the opener, per CommonMark, which is what
 * lets fenceFor() above escape a text that itself contains ``` lines. */
function parseCommitted(text) {
  const lines = normalize(text).split('\n');
  const rows = [];
  const sections = new Map();
  const textless = [];
  const summary = new Map();

  let section = null;
  let currentId = null;
  let body = []; /* every line of the current section, fence contents included */
  let payload = []; /* only the lines between the current section's first fence pair */
  let fence = null; /* the open fence's backticks, or null when outside a fence */

  const flush = () => {
    if (currentId !== null) sections.set(currentId, { body: body.join('\n'), payload: payload.join('\n') });
    currentId = null;
    body = [];
    payload = [];
  };

  for (const line of lines) {
    const fenceMatch = line.match(/^(`{3,})\s*$/);
    if (fence !== null) {
      body.push(line);
      if (fenceMatch && fenceMatch[1].length >= fence.length) fence = null;
      else payload.push(line);
      continue;
    }
    if (fenceMatch && currentId !== null && payload.length === 0) {
      fence = fenceMatch[1];
      body.push(line);
      continue;
    }

    if (line.startsWith('## ')) {
      flush();
      section = line.slice(3).trim();
      continue;
    }
    if (section === 'License texts' && line.startsWith('### ')) {
      flush();
      currentId = line.slice(4).trim();
      continue;
    }
    if (currentId !== null) {
      body.push(line);
      continue;
    }
    /* Parsed here, inside the fence-aware loop, rather than by a regex over the whole file: the MPL
     * text is itself Markdown and a bare scan would happily read a bullet out of a license body. */
    if (section === 'Summary') {
      const m = line.match(/^- (.+): \*\*(\d+)\*\*$/);
      if (m) summary.set(m[1], Number(m[2]));
      continue;
    }
    if (!line.startsWith('|')) continue;
    const cells = line.split('|').slice(1, -1).map((c) => c.trim());
    if (cells.length === 0 || cells[0] === 'Crate' || /^-+$/.test(cells[0])) continue;
    if (section === 'Dependencies' && cells.length === 4) {
      rows.push({ name: cells[0], version: cells[1], license: cells[2] });
    } else if (section === 'Crates that publish no license text of their own' && cells.length === 3) {
      textless.push({ name: cells[0], version: cells[1] });
    }
  }
  if (fence !== null) {
    die(
      `${path.relative(REPO_ROOT, OUTPUT)} ends inside an unterminated \`\`\` fence. The file is ` +
        'truncated or was hand-edited; regenerate it rather than trusting a partial parse.'
    );
  }
  flush();
  return { rows, sections, textless, summary };
}

/* The recorded fingerprints, read only from the header comment (everything before the first `## `
 * heading) so that a line inside a license body can never be mistaken for one. */
function parseFingerprints(normalizedContent) {
  const found = new Map();
  for (const line of normalizedContent.split('\n')) {
    if (line.startsWith('## ')) break;
    const m = line.match(FINGERPRINT_LINE_RE);
    if (m) found.set(m[1], m[2].trim());
  }
  return found;
}

/* OFFLINE BY CONSTRUCTION: nothing below spawns cargo, touches the network, or reads the registry.
 * The only subprocess is git, for the input fingerprint. */
function check() {
  const rel = path.relative(REPO_ROOT, OUTPUT);
  if (!fs.existsSync(OUTPUT)) {
    die(`${rel} does not exist. Generate it: node scripts/gen-third-party-notices.mjs`);
  }
  const raw = fs.readFileSync(OUTPUT, 'utf8');
  const content = normalize(raw);
  const committed = parseCommitted(raw);
  const recorded = parseFingerprints(content);

  const problems = [];

  /* --- The table's own coherence, checked before the digests so that a hand edit fails by name --- */

  if (committed.rows.length === 0) {
    problems.push(
      'the Dependencies table is empty or unparseable. Every comparison below is a set difference, ' +
        'and a difference against an empty set would otherwise read as agreement.'
    );
  }

  const rowKeys = new Set(committed.rows.map((r) => `${r.name} ${r.version}`));
  if (rowKeys.size !== committed.rows.length) {
    problems.push(
      `the Dependencies table has ${committed.rows.length} rows but only ${rowKeys.size} distinct ` +
        'crate/version pairs - a duplicated row means the table was edited by hand'
    );
  }

  /* Identifiers the TABLE declares, parsed out of its own SPDX column. With cargo gone this is the
   * expected set, and it is the right one: the sections exist to serve the table. */
  const tableIds = new Set();
  for (const r of committed.rows) for (const id of licenseIds(r.license)) tableIds.add(id);
  const ids = [...tableIds].sort();

  const committedIds = [...committed.sections.keys()].sort();
  for (const id of ids) {
    if (!committed.sections.has(id)) problems.push(`no "### ${id}" license-text section for an identifier the dependency table declares`);
  }
  for (const id of committedIds) {
    if (!tableIds.has(id)) problems.push(`license-text section "### ${id}" is stale - no row in the dependency table declares it`);
  }

  for (const id of committedIds) {
    const { body, payload } = committed.sections.get(id);
    if (body.includes(UNAVAILABLE_MARKER)) continue;
    if (payload.length < MIN_TEXT_CHARS) {
      problems.push(
        `the "### ${id}" section carries no license body (${payload.length} chars; ${MIN_TEXT_CHARS} required) ` +
          `and does not declare "${UNAVAILABLE_MARKER}" either - a section that is neither is a silent omission`
      );
    }
  }

  /* The textless list must be a subset of the table - that catches the stale entry a version bump
   * leaves behind, and the invented crate a hand edit adds. */
  for (const t of committed.textless) {
    if (!rowKeys.has(`${t.name} ${t.version}`)) {
      problems.push(`"publish no license text" lists ${t.name} ${t.version}, which is not a row in the dependency table`);
    }
  }

  /* The Summary is generated FROM the same three collections, so any disagreement is an edit. This
   * is what makes a deleted or duplicated table row fail by name rather than only by digest. */
  const counted = [
    ['Third-party crates linked', committed.rows.length, 'rows in the Dependencies table'],
    ['Distinct license identifiers', tableIds.size, 'distinct SPDX identifiers in that table'],
    ['Crates whose published package carries no license text of its own', committed.textless.length, 'rows in the textless table'],
  ];
  for (const [label, actual, what] of counted) {
    if (!committed.summary.has(label)) {
      problems.push(`the Summary has no "- ${label}: **N**" line; this file's own header is malformed`);
    } else if (committed.summary.get(label) !== actual) {
      problems.push(`the Summary claims ${committed.summary.get(label)} for "${label}" but there are ${actual} ${what}`);
    }
  }

  /* --- The two digests ---------------------------------------------------------------------- */

  for (const name of FINGERPRINT_NAMES) {
    if (!recorded.has(name)) {
      problems.push(
        `no "fingerprint ${name}" line in the header comment. Without it this file cannot be verified ` +
          'offline at all, and a check that cannot run must never pass'
      );
    }
  }

  const bodyRecorded = recorded.get('generated-body');
  if (bodyRecorded) {
    const bodyActual = bodyDigest(content);
    if (bodyActual !== bodyRecorded) {
      problems.push(
        `generated-body digest mismatch (recorded ${bodyRecorded.slice(0, 16)}..., actual ` +
          `${bodyActual.slice(0, 16)}...): this file's bytes have changed since it was generated. ` +
          'It is a generated file; edit the generator, not the output, and regenerate'
      );
    }
  }

  const invRecorded = recorded.get('inventory-inputs');
  let inventory = null;
  if (invRecorded) {
    inventory = inventoryFingerprint();
    if (inventory.digest !== invRecorded) {
      problems.push(
        `inventory-inputs digest mismatch (recorded ${invRecorded.slice(0, 16)}..., actual ` +
          `${inventory.digest.slice(0, 16)}...): Cargo.lock and/or a workspace Cargo.toml has changed ` +
          'since these notices were generated, so the dependency table is no longer provably current. ' +
          'Regenerate; do not edit the fingerprint'
      );
    }
  }

  if (problems.length > 0) {
    process.stderr.write(`${SELF}: ${rel} failed verification:\n`);
    for (const p of problems.sort()) process.stderr.write(`    ${p}\n`);
    process.stderr.write(`  Regenerate: node scripts/gen-third-party-notices.mjs\n`);
    process.exit(1);
  }

  /* The summary names the unavailable count instead of claiming "all texts present", and names the
   * manifest count the inventory fingerprint actually covered instead of claiming "matches cargo".
   * A green line that overstates what it checked is how a compliance gap survives review.
   *
   * ITS PREFIX IS LOAD-BEARING: scripts/check-license-shipping.sh requires the literal
   * "gen-third-party-notices: OK, offline (" in this process's output before it reports clean,
   * because exit 0 from a truncated or stubbed-out copy of this file would otherwise be read as a
   * passing check. Change the wording here and that guard's GENERATOR_OK_TOKEN must change with it. */
  const unavailable = committedIds.filter((id) => committed.sections.get(id).body.includes(UNAVAILABLE_MARKER));
  const tail =
    unavailable.length === 0
      ? 'every identifier carries a verbatim text'
      : `${unavailable.length} identifier(s) declare ${UNAVAILABLE_MARKER}: ${unavailable.join(', ')}`;
  process.stdout.write(
    `${SELF}: OK, offline (${committed.rows.length} third-party crates, ${ids.length} license identifiers; ` +
      `${tail}; both digests match over ${inventory.count} tracked manifest file(s)).\n`
  );
}

/* Named exports so scripts/test-notices-harvest.mjs can exercise the REAL harvest logic against a
 * synthetic crate tree — not a copy of it, which would test the copy. Exporting changes nothing
 * about CLI behavior; the main-guard below does the same job `require.main === module` does in CJS. */
export {
  licenseFilesOf,
  pick,
  baseName,
  hintPredicate,
  genericPredicate,
  skippedSubdir,
  plausibleSubdirLicenseFile,
  FILENAME_HINTS,
  GENERIC_LICENSE_FILE_RE,
  LICENSE_FILE_RE,
};

/* Run the CLI only when this file IS the entry point. When imported (by the harvest unit test),
 * nothing below executes — an import that regenerated THIRD-PARTY-NOTICES.md as a side effect would
 * be the bug. A wrong guard here cannot fail silent: check-license-shipping.sh refuses exit 0
 * without the "OK, offline (" token, so a --check that no-ops goes red, not green. */
const invokedAsCli = (() => {
  if (!process.argv[1]) return false;
  const argPath = path.resolve(process.argv[1]);
  const selfPath = fileURLToPath(import.meta.url);
  return process.platform === 'win32'
    ? argPath.toLowerCase() === selfPath.toLowerCase()
    : argPath === selfPath;
})();

if (invokedAsCli) {
  const args = process.argv.slice(2);
  const unknown = args.filter((a) => a !== '--check');
  if (unknown.length > 0) die(`unknown argument(s): ${unknown.join(' ')}. Usage: node scripts/gen-third-party-notices.mjs [--check]`);
  if (args.includes('--check')) check();
  else write();
}
