#!/usr/bin/env node
/**
 * Reads the version each RELEASE SURFACE is actually serving, and says whether they agree.
 *
 * ## Why this exists (2026-09-24, external review round 26, review ledger V297)
 *
 * zzop ships to four places and nothing in the repository read more than one of them. Two releases in
 * a row went out half-delivered, each in a DIFFERENT lane:
 *
 *   v0.35.0  tag + GitHub release + npm ok, MCP registry rejected it (422, a field length)
 *   v0.36.0  tag + GitHub release + MCP registry ok, npm publish failed (404, publisher auth)
 *
 * Both times the repository knew — a human wrote it in a note. That is the gap: CLAUDE.md wires
 * "read the tag off the REMOTE, never off a local `git tag`" because that misread cost this project
 * twice, and the rule has no twin for the other three surfaces. A fact only a person is holding is a
 * fact that leaves with them.
 *
 * ## Why it is hand-run and not a guard or a CI job
 *
 * It needs the network, so it cannot be a pre-commit guard without making offline commits fail. And
 * it cannot be a release-workflow gate either, because the surfaces legitimately disagree DURING a
 * release: the tag exists before the release job finishes, the release before npm publishes, npm
 * before the registry accepts. A gate there would be red on every healthy release for several
 * minutes. The honest place is a human reading it AFTER a release says it finished.
 *
 * ## What "agree" means here, and what it does not
 *
 * It compares the version STRING each surface serves. It does not verify that the bytes behind those
 * versions came from the same commit — npm provenance and the release assets could in principle
 * disagree while the strings match. That is a different check and this one does not pretend to it.
 *
 * ## Its RED is proven; its GREEN has not been observed yet
 *
 * On the day it was written this script was already red on the live state -- tag, GitHub release and
 * the MCP registry at 0.36.0, npm at 0.35.0, because that release's npm publish failed and the
 * account it publishes from is locked. So the failing path is demonstrated on a real disagreement
 * rather than a synthetic one. The passing path is not: no run of this script has yet seen four
 * surfaces agree. Worth knowing before trusting a future green -- it will be the first.
 *
 * Usage:  node scripts/measure/release-surfaces.mjs
 * Exit 1 when the surfaces disagree; exit 2 when a surface could not be read at all (which is not
 * the same as agreement and must never be reported as green).
 */
import { execFileSync } from "node:child_process";

const OWNER_REPO = "eezz4/zzop";
const NPM_PACKAGES = [
  "@zzop/cli",
  "@zzop/cli-darwin-arm64",
  "@zzop/cli-darwin-x64",
  "@zzop/cli-linux-arm64-gnu",
  "@zzop/cli-linux-x64-gnu",
  "@zzop/cli-win32-x64-msvc",
];
const MCP_NAME = "io.github.eezz4/zzop";

// The first tag `VERSIONING.md` still promises assets for. Narrowed there on 2026-09-24 (review
// ledger V304, user's call): 37 of 39 tags carry no release — 24 never had one, and the repository
// recreation took the rest — so the document now says "v0.35.0 onward" and this instrument checks
// THAT promise rather than the one that no longer exists.
//
// 🔴 Narrowing the population here is what keeps the instrument able to speak at all (V320). The
// tag check below exits before the surface-agreement check, so while it counted all 39 tags it was
// permanently red on a condition nobody intends to fix, and the DISAGREE verdict underneath it was
// unreachable — npm sat a release behind and this script could not say so. A guard that cannot be
// made green by doing what it asks teaches people to skip it; `ci-local.sh` already records that.
//
// ⚠ What this narrowing GIVES UP, stated rather than discovered: a tag at or above this line that
// loses its release is still caught, but the 37 below it are now out of scope permanently. That is
// the same trade the document took, and the two must move together — if the promise ever widens
// again, this constant is where it widens.
const PROMISED_FROM = "0.35.0";

const cmp = (a, b) => {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) if ((pa[i] || 0) !== (pb[i] || 0)) return (pa[i] || 0) - (pb[i] || 0);
  return 0;
};
const newest = (vs) => vs.filter(Boolean).sort(cmp).pop();

async function getJson(url) {
  const r = await fetch(url, { headers: { "User-Agent": "zzop-release-surfaces" } });
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.json();
}

const surfaces = [];
let allTags = [];
let releaseTags = null;
const unreadable = [];
const record = (name, version, note) => surfaces.push({ name, version, note });
const cannotRead = (name, why) => unreadable.push({ name, why });

// 1. The git tag, read off the REMOTE. A local `git tag` answers when this clone last fetched, which
//    is the misread CLAUDE.md names twice; that is why this shells out to ls-remote and not to `git tag`.
try {
  const out = execFileSync("git", ["ls-remote", "--tags", "origin"], { encoding: "utf8", maxBuffer: 8 * 1024 * 1024 });
  const tags = out
    .split("\n")
    .map((l) => l.replace(/.*refs\/tags\//, "").replace(/\^\{\}$/, ""))
    .filter((t) => /^v\d+\.\d+\.\d+$/.test(t))
    .map((t) => t.slice(1));
  if (tags.length === 0) cannotRead("git tag (remote)", "the remote advertised no release tag");
  else {
    allTags = [...new Set(tags)];
    record("git tag (remote)", newest(tags), `${allTags.length} release tag(s)`);
  }
} catch (e) {
  cannotRead("git tag (remote)", String(e.message).split("\n")[0]);
}

// 2. The GitHub releases. Reading `/releases/latest` alone was this script's own blind spot, found by
//    external review round 27 the day after it was written: `latest` is by construction the one release
//    that exists, so a repository where 37 of 39 tags have NO release at all reports a clean row. What
//    `VERSIONING.md` promises is not "the newest tag installs" but "take the assets for the TAG YOU
//    WANT" — a per-tag promise, and only the full list can see it. So read them all and compare.
try {
  const rels = await getJson(`https://api.github.com/repos/${OWNER_REPO}/releases?per_page=100`);
  releaseTags = rels.map((r) => String(r.tag_name || "").replace(/^v/, "")).filter(Boolean);
  const latest = await getJson(`https://api.github.com/repos/${OWNER_REPO}/releases/latest`);
  const v = String(latest.tag_name || "").replace(/^v/, "");
  record("GitHub release", v || null, `${(latest.assets || []).length} asset(s) on latest, ${releaseTags.length} release(s) total`);
} catch (e) {
  cannotRead("GitHub release", String(e.message));
}

// 3. npm. Every package in the set, because they are published one at a time and a partial publish is
//    exactly the failure this script was written for — reading only the shim would have shown green
//    on a release where five platform packages never landed.
const npmVersions = new Map();
for (const p of NPM_PACKAGES) {
  try {
    const d = await getJson(`https://registry.npmjs.org/${p.replace("/", "%2f")}`);
    npmVersions.set(p, d["dist-tags"]?.latest ?? null);
  } catch (e) {
    cannotRead(`npm ${p}`, String(e.message));
  }
}
if (npmVersions.size) {
  const distinct = [...new Set(npmVersions.values())];
  record(
    "npm",
    distinct.length === 1 ? distinct[0] : null,
    distinct.length === 1
      ? `${npmVersions.size} package(s), all at this version`
      : `SPLIT across ${npmVersions.size} package(s): ${[...npmVersions].map(([k, v]) => `${k}=${v}`).join(", ")}`,
  );
}

// 4. The MCP registry.
try {
  const d = await getJson(`https://registry.modelcontextprotocol.io/v0/servers?search=${encodeURIComponent(MCP_NAME)}`);
  const vs = (d.servers || []).map((s) => (s.server || s).version).filter(Boolean);
  if (vs.length === 0) cannotRead("MCP registry", "no entries returned for this server name");
  else record("MCP registry", newest(vs), `${new Set(vs).size} version(s) listed`);
} catch (e) {
  cannotRead("MCP registry", String(e.message));
}

const w = Math.max(...[...surfaces, ...unreadable].map((s) => s.name.length), 10);
for (const s of surfaces) {
  console.log(`  ${s.name.padEnd(w)}  ${String(s.version ?? "—").padStart(8)}   ${s.note}`);
}
for (const u of unreadable) {
  console.log(`  ${u.name.padEnd(w)}  ${"unread".padStart(8)}   ${u.why}`);
}

// The per-tag promise, checked separately from the version agreement above: a tag whose release does
// not exist is a `VERSIONING.md` pin that 404s, and it is invisible to a "do the surfaces agree" test
// because every surface can agree on the newest version while the history behind it is gone.
const promisedTags = allTags.filter((t) => cmp(t, PROMISED_FROM) >= 0);
const tagsWithoutRelease =
  releaseTags === null ? [] : promisedTags.filter((t) => !releaseTags.includes(t));
const outOfScope = allTags.length - promisedTags.length;
if (releaseTags !== null && allTags.length) {
  if (tagsWithoutRelease.length === 0) {
    console.log(
      `  ${"tag -> release".padEnd(w)}  ${"ok".padStart(8)}   every promised tag (>= ${PROMISED_FROM}) has a ` +
        `release; ${outOfScope} older tag(s) are out of scope, see VERSIONING.md`,
    );
  } else {
    console.log(
      `  ${"tag -> release".padEnd(w)}  ${"BROKEN".padStart(8)}   ${tagsWithoutRelease.length} of ${promisedTags.length} PROMISED tags (>= ${PROMISED_FROM}) have NO release: ` +
        tagsWithoutRelease.sort(cmp).slice(0, 6).join(", ") +
        (tagsWithoutRelease.length > 6 ? ", …" : ""),
    );
  }
}

if (unreadable.length) {
  console.error(
    `\nrelease-surfaces: ${unreadable.length} surface(s) could not be read.\n` +
      "  That is not agreement. A surface nobody could reach is the state this script exists to make\n" +
      "  visible, not a row to skip past.",
  );
  process.exit(2);
}

if (tagsWithoutRelease.length) {
  console.error(
    `\nrelease-surfaces: ${tagsWithoutRelease.length} tag(s) have no GitHub release.\n` +
      "  VERSIONING.md tells a reader to pin a tag and take that tag's assets from GitHub Releases.\n" +
      "  For these tags that instruction 404s. Agreement on the newest version does not cover it, which\n" +
      "  is why this is a separate line and a separate exit.",
  );
  process.exit(1);
}

const versions = [...new Set(surfaces.map((s) => s.version))];
if (versions.length === 1 && versions[0]) {
  console.log(`\nrelease-surfaces: OK — all ${surfaces.length} surfaces serve ${versions[0]}, and every tag has a release.`);
  process.exit(0);
}

console.error(
  `\nrelease-surfaces: the surfaces DISAGREE (${surfaces.map((s) => `${s.name}=${s.version ?? "split"}`).join(", ")}).\n` +
    "  During a release this is normal for a few minutes and means nothing. After one, it means the\n" +
    "  release is half-delivered: some consumers are on the new version and some are not, and which\n" +
    "  ones depends on how they install. Check the release workflow's last run before assuming a\n" +
    "  registry is at fault.",
);
process.exit(1);
