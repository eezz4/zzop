#!/usr/bin/env bash
# check-marker-claims — when a rule message tells a user WHICH comment leader carries its suppress
# marker, that sentence must agree with the engine table that actually decides it.
#
# ## The defect this exists for (F2, 2026-08-12, `d00b6ad` -> `c0cc8ed`)
#
# `d00b6ad` added `"py"` to `crates/core/src/dsl/markers/path.rs::HASH_COMMENT_EXTENSIONS`, so `#` began
# carrying suppress markers in Python. It rewrote the message of every line-scan rule it FOUND admitting
# `.py` — but not `sql/destructive-migration`, which went on telling readers that NO line marker is
# writable in a `.py` migration and that the only lever left is a whole-file `rules` config entry.
#
# The roster and its size are not retyped here: the `.py` entry in `path.rs` owns both and carries the
# recount command. What belongs here is WHY the miss happened, because it is this guard`s failure mode one
# level down. `d00b6ad` recounted by EXECUTING each `file_pattern` rather than grepping it — the right
# method, adopted mid-commit after a text scan undercounted — but it executed against BARE probe paths
# (`a.py`, `a.pyi`). `destructive-migration` is anchored under `migrations/`|`migrate/`|`alembic/versions/`,
# so no bare path can reach it and it fell out of a count that was otherwise measured rather than guessed.
# The rule was in the bundle at that moment and stayed there for another thirteen hours (`9a49080`
# exported it, `c0cc8ed` brought it back with the message fixed), so being outside the bundle explains
# nothing: a needle narrower than the claim it serves reads nothing of what it cannot reach.
#
# The message was false for a whole rule family and every existing guard was green: this is not a rule
# ID (`check-prose-rule-ids.sh`), not a config key (`check-doc-config-keys.sh`), not a duplicate of the
# engine-appended sentence (`check-pack-suppress-sentence.sh`) — it is a true-or-false claim about engine
# behaviour, sitting in a string the user reads, with no reader on either side.
#
# `channel.rs::suppress_hint` states the convention in as many words — the engine-appended sentence
# names `//` only, and "a rule whose `file_pattern` targets those families should say so in its own
# `message`". CHECK 2 below is that sentence with a checker attached. Until now it had none, which is
# why every sibling complied and one rule did not, for the seventeen hours between `d00b6ad` and
# `c0cc8ed`.
#
# ## The three checks, and why none of them needs an allowlist
#
# The engine table is READ OUT OF `path.rs`, never retyped here: the hash-comment extension list and the
# `.sql` arm are parsed from the source, and this guard aborts if either shape stops being findable —
# a copy of that table would be one edit away from vouching for the wrong answer, which is the exact
# defect above one level up. Which extensions a rule reads is DERIVED the same way: its `file_pattern`
# is executed against probe paths rather than read by eye.
#
#   1. NO FALSE PROMISE. Every `` `<leader> zzop-<id>-ok` `` in a message is scoped by the extensions
#      named alongside it (nearest promise-to-promise span inside its own sentence, forward first, then
#      backward — the shape `... `-- <marker>` in a `.sql` file), or `# <marker>` in a `.py` file.`
#      resolves exactly). An unscoped promise is a claim about EVERY file the rule reads, and is judged
#      that way. `//` is skipped: the engine grants it everywhere, so it cannot be falsified.
#   2. NO MISSING WIDENING. If a rule reads a file type where the engine honours `#` or `--`, its
#      message must name that form somewhere. A rule that says nothing gets the engine`s `//`-only
#      sentence, and `// zzop-x-ok` is a SyntaxError in Python and a stray line in YAML — the reader is
#      told to write the one comment that cannot work. This is F2 exactly.
#   3. NO AXIS FUSION. `path.rs` holds TWO tables for the same file: `marker_leaders_for_path` (can this
#      comment carry a marker) and `leaders_for_path` (is this line commentary to skip). For `.py` they
#      DISAGREE on purpose — `#` suppresses, but a `#`-commented-out DELETE is still read as live code,
#      and a `#`-commented secret is still a committed secret. A message that describes leaders with one
#      fused term ("the comment/marker leader") is therefore true of at most one of the two tables. Three
#      live rules said exactly that while promising a `#` marker four sentences later, i.e. contradicted
#      themselves inside one message.
#
# Measured on the clean tree (2026-08-12): 136 marker-bearing rules across 15 packs, 11 non-`//`
# promises judged, 0 offenders. Run against the `9a49080` tree (F2 alive) it reports FIVE, and the one
# that is F2 itself names the rule, the extension and the form the message owes a reader:
# `destructive-migration` (printed under `sql-preferences/` on that tree, back in `sql/` since `c0cc8ed`)
# reads `.py`, the engine honours `# zzop-...-ok` there, and the message never named that form. The
# other four are the three axis-fusion sentences and the `.pyi` promise this commit repaired — live on
# that tree, live on this one until they were fixed.
#
# ## What this deliberately does NOT check
#
# 1. WHETHER THE LEADER IS WRITABLE IN THAT LANGUAGE. `//` is granted for every file, so a promise of
#    `//` can never be false here — even in a `.pyi` stub, where it is granted and unspellable. That gap
#    is real and was found by CHECK 1 through the OTHER half of the claim, not by a rule about `//`.
# 2. THE SKIP AXIS. Whether `skip_comment_lines` honours a leader is a separate table, and a message
#    that misstates it is not detectable by leader/extension token shape — CHECK 3 catches only the
#    fused wording that claims both tables at once.
# 3. PROSE THAT MAKES THE SAME CLAIM IN OTHER WORDS. "No line comment can suppress this in Python" holds
#    no leader token and no marker string, so nothing here sees it. What is caught is the spelled
#    promise and its absence; an English paraphrase of a false promise is a domain review`s job.
# 4. SHIPPED PROSE, i.e. docs/**. The subject here is a rule `message` in pack JSON, and widening it to
#    Markdown was measured (2026-08-12) and DECLINED rather than skipped, so the next reader does not
#    re-derive the same dead end. `d00b6ad` put `py` in the table and left TWO false sentences alive in
#    docs/rules/dsl-reference.md for 19 hours, which is the same defect class as F2 one surface over.
#    Only ONE of the two is reachable by any needle this guard could carry:
#      * "`.py`/`.sh`/`.rb` are deliberately NOT in the family" spells an extension token, so an
#        extension-vs-table test sees it.
#      * "Python is not in the config-file family" spells the LANGUAGE, not the extension. It carries no
#        leader token, no marker string and no `.ext` token — nothing the table can be compared against.
#    A prose leg would therefore report clean on half of a defect it was built for, and buy that with a
#    paragraph-anchoring stack (which Markdown section is the claim about?) whose own failure mode is
#    attaching to the wrong set silently. The complete-roster rule check-framework-prose-enumeration.sh
#    uses does not rescue it either: that rule counts NAMES, and the stale paragraph named `.py` — inside
#    the negation — so it would have passed green on the live falsehood. The prose roster is instead
#    marked in dsl-reference.md as a second copy with `HASH_COMMENT_EXTENSIONS` named as its owner, and
#    keeping it true is a domain review`s job, not this guard`s.
# 4. `file_exclude_pattern` and non-extension path shapes. Admission is decided by extension probes, so
#    a rule matching e.g. an extensionless `Dockerfile` contributes no admitted extension and its
#    unscoped promises are judged against whatever else it reads.
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v node >/dev/null 2>&1; then
  echo "check-marker-claims: \`node\` not found. This guard parses pack JSON and executes each rule" >&2
  echo "  file_pattern as a regex, so it cannot degrade to grep. Install node; skipping would report" >&2
  echo "  clean while checking nothing." >&2
  exit 1
fi

node --input-type=module -e '
import fs from "fs";
import path from "path";

// --- the engine table, READ from path.rs -------------------------------------------------------
const PATH_RS = "crates/core/src/dsl/markers/path.rs";
const pathRs = fs.readFileSync(PATH_RS, "utf8");
const constM = /const HASH_COMMENT_EXTENSIONS:\s*&\[&str\]\s*=\s*&\[([^\]]*)\]/.exec(pathRs);
if (!constM) {
  console.error("check-marker-claims: could not find `HASH_COMMENT_EXTENSIONS` in " + PATH_RS + ".");
  console.error("  That constant IS the table this guard judges against; without it every `#` promise");
  console.error("  would read as false. Re-point the extraction rather than deleting the check.");
  process.exit(1);
}
const hashExts = new Set([...constM[1].matchAll(/"([a-z0-9]+)"/g)].map((m) => m[1]));
const sqlArm = /eq_ignore_ascii_case\("sql"\)[\s\S]{0,120}Leaders::SlashOrSql/.test(pathRs);
if (hashExts.size < 5 || !sqlArm) {
  console.error("check-marker-claims: read " + hashExts.size + " hash-comment extension(s) and " +
    (sqlArm ? "the" : "NO") + " `.sql` arm from " + PATH_RS + " (9 and one, on 2026-08-12). The table");
  console.error("  shape changed; a guard that judges against a half-read table is worse than none.");
  process.exit(1);
}
// PerFileText is the ONLY channel that consults `marker_leaders_for_path`; the multi-language and
// io-scan channels are leader-neutral and symbol-scan has no anchor line. Pinned against its owner so
// a re-mapped kind cannot silently drop out of this guard subject.
const CHANNEL_RS = "crates/core/src/dsl/markers/channel.rs";
if (!/Matcher::LineScan\(_\) \| Matcher::MethodScan\(_\) => MarkerChannel::PerFileText/.test(fs.readFileSync(CHANNEL_RS, "utf8"))) {
  console.error("check-marker-claims: " + CHANNEL_RS + " no longer maps LineScan|MethodScan to");
  console.error("  PerFileText. That mapping is what makes line-scan/method-scan the per-FILE marker");
  console.error("  subject; re-derive the subject set before trusting this guard again.");
  process.exit(1);
}
const PER_FILE_KINDS = new Set(["line-scan", "method-scan"]);
const granted = (ext) => (ext === "sql" ? ["//", "--"] : hashExts.has(ext) ? ["//", "#"] : ["//"]);

// --- packs, bundled and exported alike ----------------------------------------------------------
// Both are subjects: the marker table does not ask which pack a rule shipped in, and F2 was live in an
// EXPORTED pack when it was written.
const packsUnder = (dir) => {
  const out = [];
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name).split(path.sep).join("/");
      if (e.isDirectory()) { walk(p); continue; }
      if (!e.name.endsWith(".json")) continue;
      const pack = JSON.parse(fs.readFileSync(p, "utf8"));
      if (!pack.id || !Array.isArray(pack.rules)) continue;
      out.push({ file: p, id: pack.id, rules: pack.rules });
    }
  };
  walk(dir);
  return out;
};
const packs = [...packsUnder("rules/dsl"), ...packsUnder("examples/packs")];
if (packs.length < 10) {
  console.error("check-marker-claims: enumerated " + packs.length + " pack(s) (15 on 2026-08-12). An");
  console.error("  empty or shrunken subject set is a broken guard, never a clean tree.");
  process.exit(1);
}

// Candidate extensions: the table`s own families plus every lowercase word any file_pattern spells.
// Over-generation is safe — a candidate survives only if the pattern itself accepts a path carrying it.
const candidates = new Set([...hashExts, "sql"]);
for (const p of packs) for (const r of p.rules) {
  for (const m of ((r.matcher && r.matcher.file_pattern) || "").matchAll(/[a-z][a-z0-9]{0,9}/g)) candidates.add(m[0]);
}
const PROBE_DIRS = ["", "src/", "migrations/", "migration/", "migrate/", "alembic/versions/", "config/",
  "src/api/", "src/routes/", "src/controllers/", "src/domains/x/routes/", "src/x.handler/"];
// Rust `regex` has no lookaround, so its patterns are a subset of JS syntax — apart from the inline
// `(?i)` flag, which JS spells as a flag instead.
const toJsRegex = (src) => {
  let flags = "", s = src;
  if (s.startsWith("(?i)")) { flags = "i"; s = s.slice(4); }
  s = s.replace(/\(\?i\)/g, "");
  try { return new RegExp(s, flags); } catch { return null; }
};

const offenders = [];
let rulesJudged = 0, promisesJudged = 0;

for (const pack of packs) {
  for (const rule of pack.rules) {
    const kind = rule.matcher && rule.matcher.type;
    if (!PER_FILE_KINDS.has(kind)) continue;
    rulesJudged++;
    const where = pack.file + "  rule `" + pack.id + "/" + rule.id + "`";
    const fp = rule.matcher.file_pattern;
    const admitted = new Set();
    if (!fp) {
      for (const e of candidates) admitted.add(e); // no pattern: the rule reads every file
    } else {
      const re = toJsRegex(fp);
      if (!re) {
        offenders.push(where + "  — file_pattern does not compile as a regex here, so the extensions" +
          " this rule reads cannot be derived and no claim about them can be judged.");
        continue;
      }
      for (const e of candidates) for (const d of PROBE_DIRS) if (re.test(d + "f." + e)) admitted.add(e);
      for (const d of PROBE_DIRS) if (re.test(d + ".env") || re.test(d + ".env.local")) admitted.add("env");
    }

    const msg = rule.message || "";
    const marker = "zzop-" + rule.id + "-ok";
    const promiseRe = new RegExp("`(//|--|#)\\s*" + marker.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + "`", "g");
    const promises = [...msg.matchAll(promiseRe)].map((m) => ({ leader: m[1], start: m.index, end: m.index + m[0].length }));

    // Clause bounds. A period or semicolon followed by whitespace never sits inside an extension token
    // (`.py` has no space after the dot), so this split cannot cut a scope in half. `;` counts because a
    // suppression sentence routinely carries one clause per file type ("... in a `.sql` file); in a
    // `.py` migration ..."), and running a scope past it would credit one clause`s leader to the next
    // clause`s extension.
    const bounds = [0, ...[...msg.matchAll(/[.;]\s+/g)].map((m) => m.index + m[0].length), msg.length];
    const sentenceOf = (pos) => {
      let s = 0, e = msg.length;
      for (const b of bounds) { if (b <= pos) s = b; else { e = b; break; } }
      return [s, e];
    };
    const extsIn = (a, b) => {
      const out = new Set();
      for (const m of msg.slice(a, b).matchAll(/`?\.([a-z][a-z0-9]{0,8})`?/g)) if (candidates.has(m[1])) out.add(m[1]);
      return out;
    };

    // --- CHECK 1 --------------------------------------------------------------------------------
    for (let i = 0; i < promises.length; i++) {
      const p = promises[i];
      if (p.leader === "//") continue;
      promisesJudged++;
      const [ss, se] = sentenceOf(p.start);
      const fwdEnd = Math.min(se, i + 1 < promises.length ? promises[i + 1].start : se);
      const bwdStart = Math.max(ss, i > 0 ? promises[i - 1].end : ss);
      let scope = extsIn(p.end, fwdEnd);
      if (scope.size === 0) scope = extsIn(bwdStart, p.start);
      const scoped = scope.size > 0;
      const targets = scoped ? [...scope] : [...admitted];
      const ungranted = targets.filter((e) => !granted(e).includes(p.leader));
      if (ungranted.length) {
        offenders.push(where + "  — promises `" + p.leader + " " + marker + "`" +
          (scoped ? " for " : " with no extension named, so for every file it reads — including ") +
          ungranted.map((e) => "." + e + " (engine honours " + granted(e).map((g) => "`" + g + "`").join(" and ") + " there)").join(", ") +
          ". Table: " + PATH_RS + ".");
      }
      const unread = scoped ? [...scope].filter((e) => !admitted.has(e)) : [];
      if (unread.length) {
        offenders.push(where + "  — promises `" + p.leader + " " + marker + "` for " +
          unread.map((e) => "." + e).join("/") + ", which this rule file_pattern never reads. A marker" +
          " offered for files the rule cannot fire on is a claim with no subject.");
      }
    }

    // --- CHECK 2 --------------------------------------------------------------------------------
    const promised = new Set(promises.map((p) => p.leader));
    for (const e of [...admitted].sort()) {
      for (const l of granted(e)) {
        if (l === "//" || promised.has(l)) continue;
        offenders.push(where + "  — reads ." + e + ", where the engine honours `" + l + " " + marker +
          "`, but the message never names that form. The sentence a reader gets is the engine-appended" +
          " `// " + marker + "`, which is not a comment in that file type.");
      }
    }

    // --- CHECK 3 --------------------------------------------------------------------------------
    const splitAxis = [...admitted].filter((e) => e !== "sql" && granted(e).length > 1);
    const fused = /comment\s*\/\s*marker|marker\s*\/\s*comment|comment (?:or|and) marker|marker (?:or|and) comment/i.exec(msg);
    if (fused && splitAxis.length) {
      offenders.push(where + "  — says \"" + fused[0] + "\" while reading " +
        splitAxis.map((e) => "." + e).join("/") + ", where the MARKER table and the SKIP table" +
        " deliberately disagree. One term for both is true of at most one of them; name the axis you" +
        " mean, as sql/delete-no-where does.");
    }
  }
}

if (rulesJudged < 60 || promisesJudged < 5) {
  console.error("check-marker-claims: judged " + rulesJudged + " marker-bearing rule(s) and " +
    promisesJudged + " non-`//` promise(s) (136 and 11 on 2026-08-12). An extraction stopped matching;");
  console.error("  this is a broken guard reporting clean, not a repo that stopped making claims.");
  process.exit(1);
}

if (offenders.length) {
  console.error("check-marker-claims: rule messages disagree with the engine marker table:");
  for (const o of offenders) console.error("  " + o);
  process.exit(1);
}

console.log("check-marker-claims: OK (" + rulesJudged + " marker-bearing rule(s) across " + packs.length +
  " packs, " + promisesJudged + " non-`//` leader promise(s), engine table read from " + PATH_RS + ")");
'
