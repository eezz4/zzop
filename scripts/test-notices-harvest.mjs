/* Unit fixture for the license-harvest logic in scripts/gen-third-party-notices.mjs.
 *
 * WHY THIS EXISTS
 * check-license-shipping.sh --check verifies the COMMITTED notices file against digests the
 * generator itself stamped at write time. That closes hand edits and stale inventories; it closes
 * nothing about the harvest: a regression in licenseFilesOf()/pick() regenerates wrong license
 * BODIES with a fresh, self-consistent digest, and every gate stays green. The two named hazards
 * this file pins:
 *   - a "consistency" refactor that basenames the generic-pass match would let a subdirectory's
 *     bare LICENSE (vendored third-party code's own license) into the generic pool;
 *   - the one-level descent, un-narrowed, admits vendor/ / third_party/ license files as hint-pass
 *     candidates for the crate's OWN license.
 *
 * REGISTRY-FREE BY CONSTRUCTION: builds a synthetic crate tree in a temp dir and imports the REAL
 * licenseFilesOf/pick/FILENAME_HINTS from the generator (named exports; importing runs no CLI --
 * the generator is main-guarded). No cargo, no git, no network. Exit 1 on the first violated
 * assertion, naming the case.
 *
 * ITS SUCCESS LINE IS LOAD-BEARING: scripts/check-license-shipping.sh requires the literal
 * "test-notices-harvest: OK (" in this process's output, because exit 0 from a truncated or
 * stubbed-out copy of this file would otherwise read as a passing test.
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import {
  licenseFilesOf,
  pick,
  hintPredicate,
  genericPredicate,
  skippedSubdir,
  plausibleSubdirLicenseFile,
  FILENAME_HINTS,
} from './gen-third-party-notices.mjs';

const SELF = 'test-notices-harvest';
let assertions = 0;

function fail(caseName, detail) {
  process.stderr.write(`${SELF}: FAIL -- ${caseName}: ${detail}\n`);
  process.exitCode = 1;
  throw new Error(caseName);
}

function assert(cond, caseName, detail) {
  assertions += 1;
  if (!cond) fail(caseName, detail);
}

/* Pad license bodies past any plausible length floor so this fixture never couples to
 * MIN_TEXT_CHARS. The marker string is what the assertions actually look for. */
const body = (marker) => `${marker}\n${'lorem ipsum dolor sit amet '.repeat(20)}\n`;

function plant(root, files) {
  for (const [rel, content] of Object.entries(files)) {
    const p = path.join(root, ...rel.split('/'));
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, content, 'utf8');
  }
}

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'zzop-notices-harvest-'));
try {
  /* Crate A: a root license, a legitimate one-level-down license (the witnessed unicode_names2
   * shape), and every trap at once. */
  const dirA = path.join(tmp, 'fake-crate-1.0.0');
  plant(dirA, {
    'Cargo.toml': '[package]\nname = "fake-crate"\nversion = "1.0.0"\n',
    'LICENSE-MIT': body('ROOT-MIT-TEXT'),
    'data/LICENSE-UNICODE': body('DATA-UNICODE-TEXT'), /* legitimate: data/ is not skipped */
    'vendor/LICENSE-MIT': body('VENDORED-MIT-TEXT'), /* vendored project's own license */
    'limits/LICENSE.txt': body('LIMITS-TEXT'), /* dir NAME matches /mit/i; basename must decide */
    'docs/LICENSE': body('SUBDIR-BARE-LICENSE'), /* bare LICENSE one level down */
    'src/license_mit.rs': body('RUST-SOURCE-NOT-A-LICENSE'), /* license-prefixed source file */
  });
  const pkgA = { id: 'fake-crate 1.0.0', name: 'fake-crate', version: '1.0.0', manifest_path: path.join(dirA, 'Cargo.toml') };

  /* Crate B: NO root license. Only the traps remain, so a pass that reaches into them has nothing
   * legitimate to shadow the bug. */
  const dirB = path.join(tmp, 'trap-crate-2.0.0');
  plant(dirB, {
    'Cargo.toml': '[package]\nname = "trap-crate"\nversion = "2.0.0"\n',
    'vendor/LICENSE-MIT': body('VENDORED-MIT-TEXT-B'),
    'limits/LICENSE.txt': body('LIMITS-TEXT-B'),
    'docs/LICENSE': body('SUBDIR-BARE-LICENSE-B'),
  });
  const pkgB = { id: 'trap-crate 2.0.0', name: 'trap-crate', version: '2.0.0', manifest_path: path.join(dirB, 'Cargo.toml') };

  /* ---- The harvested candidate sets ---------------------------------------------------------- */
  const infoA = licenseFilesOf(pkgA);
  const infoB = licenseFilesOf(pkgB);
  assert(infoA.readable && infoB.readable, 'fixture-readable', 'licenseFilesOf reported the fixture dirs unreadable');

  assert(infoA.files.includes('LICENSE-MIT'), 'root-license-harvested',
    `root LICENSE-MIT missing from candidates: [${infoA.files}]`);
  assert(infoA.files.includes('data/LICENSE-UNICODE'), 'data-subdir-still-covered',
    `data/LICENSE-UNICODE (the witnessed unicode_names2-1.3.0 shape) missing from candidates: [${infoA.files}] -- the descent narrowing over-reached`);
  assert(!infoA.files.includes('vendor/LICENSE-MIT'), 'vendor-subdir-skipped',
    `vendor/LICENSE-MIT is in the candidate pool: [${infoA.files}] -- a vendored project's license is a candidate for the crate's own`);
  assert(!infoA.files.includes('src/license_mit.rs'), 'extension-constraint',
    `src/license_mit.rs is in the candidate pool: [${infoA.files}] -- a .rs source file passed as a plausible license text`);
  assert(infoA.files.includes('limits/LICENSE.txt') && infoA.files.includes('docs/LICENSE'),
    'non-skipped-subdirs-harvested',
    `limits/LICENSE.txt and docs/LICENSE should be CANDIDATES (their exclusion is the hint/generic passes' job, not the harvest's): [${infoA.files}]`);
  assert(infoA.files[0] === 'LICENSE-MIT', 'root-sorts-first',
    `root files must sort before subdirectory files within one crate, got [${infoA.files}]`);

  /* ---- The hint pass, exactly as derive() runs it -------------------------------------------- */
  /* `hintPredicate` is THE function derive() passes to pick(), imported, not rebuilt. It was
   * rebuilt here until 2026-08-01, and a mutation test proved what that cost: changing derive()'s
   * call site to match on the full path instead of the basename left this file GREEN while it
   * printed a success line naming that exact hazard. A fixture that reconstructs its subject tests
   * the reconstruction. */
  const hintPred = hintPredicate(FILENAME_HINTS.MIT);
  const perCrate = new Map([[pkgA.id, infoA], [pkgB.id, infoB]]);

  const chosenA = pick([pkgA], perCrate, hintPred);
  assert(chosenA !== null && chosenA.source === 'fake-crate-1.0.0/LICENSE-MIT',
    'hint-picks-root-license',
    `MIT hint on crate A picked ${chosenA && chosenA.source}, expected fake-crate-1.0.0/LICENSE-MIT`);
  assert(chosenA.text.includes('ROOT-MIT-TEXT') && !chosenA.text.includes('VENDORED'),
    'hint-pick-body-is-roots',
    `MIT hint returned a body that is not the root license's: ${chosenA.text.slice(0, 60)}...`);

  const chosenB = pick([pkgB], perCrate, hintPred);
  assert(chosenB === null, 'hint-matches-basename-not-dir',
    `MIT hint on crate B (no root license) picked ${chosenB && chosenB.source} -- either the /mit/i hint ` +
      'matched the limits/ DIRECTORY name instead of the basename, or a vendor/ file leaked into the pool');

  /* ---- The generic pass ---------------------------------------------------------------------- */
  const chosenGenericB = pick([pkgB], perCrate, genericPredicate);
  assert(chosenGenericB === null, 'generic-pass-never-descends',
    `the generic pass picked ${chosenGenericB && chosenGenericB.source} from crate B -- a subdirectory's bare ` +
      'LICENSE (the vendored-third-party shape) satisfied the generic predicate. It must stay anchored on ' +
      'the full relative path, never the basename');

  /* ---- The two gate predicates, driven directly ----------------------------------------------
   * Both decide which license BODY ships. Their failures are invisible at runtime: a skipped-subdir
   * miss reproduces somebody else's license under this crate's heading, and an over-narrow file
   * test degrades an identifier to NO LOCAL LICENSE TEXT FOUND, which check() steps past rather
   * than failing on. Neither is reachable through licenseFilesOf() alone for every case, so they
   * are pinned here as values. */
  for (const dir of ['vendor', 'third_party', 'third-party', 'Third-Party', 'VENDORED']) {
    assert(skippedSubdir(dir), 'skipped-subdir-spellings',
      `skippedSubdir(${dir}) must be true -- the hyphen and underscore spellings of one directory ` +
        'name must not disagree; third-party/ walked past an underscore-only list until 2026-08-01');
  }
  for (const dir of ['data', 'licenses', 'legal', 'doc']) {
    assert(!skippedSubdir(dir), 'skipped-subdir-not-overbroad',
      `skippedSubdir(${dir}) must be false -- unicode_names2 ships its only license text under data/`);
  }
  for (const name of ['LICENSE-APACHE-2.0', 'LICENSE-BSD-3.0', 'COPYING.LESSER', 'LICENSE.rst', 'LICENSE-MIT']) {
    assert(plausibleSubdirLicenseFile(name), 'subdir-license-spellings-kept',
      `plausibleSubdirLicenseFile(${name}) must be true -- an allow-list of (extensionless|.md|.txt) ` +
        'dropped all four of these, including the version-dotted spellings its own comment cited');
  }
  for (const name of ['license_mit.rs', 'licence.js', 'LICENSE.png']) {
    assert(!plausibleSubdirLicenseFile(name), 'subdir-source-still-excluded',
      `plausibleSubdirLicenseFile(${name}) must be false -- src/license_mit.rs satisfies the MIT hint ` +
        'by basename and is the reason this gate exists');
  }
} catch (e) {
  /* An assertion failure already wrote its named FAIL line and set the exit code; rethrowing it
   * would only bury that line under a stack trace. Anything ELSE is a genuine crash and must
   * propagate loudly -- a crash mistaken for a tidy failure is a hidden class of its own. */
  if (!process.exitCode) throw e;
} finally {
  fs.rmSync(tmp, { recursive: true, force: true });
}

if (process.exitCode) process.exit(process.exitCode);
process.stdout.write(
  `${SELF}: OK (${assertions} assertions over a synthetic crate tree: vendor/ skipped, data/ covered, ` +
    'extension constraint holds, hint matches basename only, generic pass never descends).\n'
);
