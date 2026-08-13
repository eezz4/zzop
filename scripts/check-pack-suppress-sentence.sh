#!/usr/bin/env bash
# Pack-message suppress-sentence guard — fails when a `rules/dsl/*/*.json` rule hand-writes the
# suppress-marker sentence the ENGINE already appends to that rule's findings.
#
# ## What this exists to stop
# Until 2026-08-09, 106 of 143 shipped rules ended their own `message` with the SAME 44 bytes:
#
#     Suppress a vetted case with `// zzop-<id>-ok`.
#
# Not similar — byte-identical modulo the rule id, which the sentence derives rather than adds. That
# text now comes from `zzop_core::dsl::suppress_hint`, appended at finding-construction time by
# `zzop-engine`'s `pipeline::findings::append_hints`. Nothing about deleting 106 copies stops the 107th
# from being typed; a fold with no guard is a fold with a refill rate. This is that guard.
#
# There is an exact precedent, and it is deliberate that this file mirrors it: when the config
# DISABLE hint was folded into the same append, the positive leg moved to the append point and
# `crates/engine/tests/rule_contracts/dsl_messages.rs`'s contract 17
# (`no_dsl_pack_message_hand_writes_the_engine_appended_disable_hint`) was added to forbid authors
# writing that sentence into a pack message. Same shape, one axis over.
#
# ## The rule, stated exactly — and why it is not "grep for the sentence"
# A bare "no message may contain this sentence" would be WRONG in both directions, measured against
# this very tree:
#
#   * FALSE POSITIVE on `security/insecure-cookie`, whose copy sits MID-message with more text after
#     it. Its finding renders correctly today: `suppress_hint` opts out when the author already named
#     the marker, so the engine appends nothing and there is no duplication to remove. Folding it
#     would MOVE the sentence to the tail, i.e. change bytes for no gain.
#   * FALSE POSITIVE on the four `call-scan` rules (`code-hygiene/console-in-be`,
#     `code-hygiene/console-in-loop`, `code-hygiene/env-outside-config`, `security/weak-password-hash`).
#     They end with the `#`-disclosing variant, which IS a sentence the engine can emit — but each of
#     them names its marker a SECOND time earlier in the message, so the opt-out fires and the engine
#     appends nothing. Delete their tail sentence and the finding LOSES it. Their copy is load-bearing.
#   * FALSE NEGATIVE on `security/config-file-secret`, which correctly writes a `#`-leader sentence
#     the engine never emits for a line-scan rule. Nothing here may push an author toward `//` in a
#     `.env` file, where `//` is not a comment at all.
#
# So the test is not "does this text appear" but "is this text REDUNDANT", and redundancy is decided
# by simulating the engine: strip the trailing sentence, then ask whether the append would have put it
# back. If it would, the author typed something they get for free. If it would not (the marker is named
# elsewhere, or the matcher kind gets no sentence at all), the copy is the only one there is and stays.
#
# Anchored to the message TAIL, never to a substring anywhere: the tail is the position the append
# writes to, so it is the only position at which a hand-written copy is the same string in the same
# place. That is also what keeps `insecure-cookie` out of scope without an exemption list — this guard
# has none, and must not grow one.
#
# ## Why node rather than grep/awk
# Pack JSON is mostly regex source (`"pattern": "[\"']lazy[\"']\\s*:..."`), so a line/substring scan
# for `"message"` reads quoting it has no parser for. Contract 17 records that this mistake has already
# been made once in this repo, which is why IT walks `serde_json` rather than bytes. `node` is a hard
# dependency of the guard fleet already (see check-rules-catalog-sync.sh's own note) and is required
# here for the same reason: a guard that skips itself when a tool is missing is a guard that reports
# clean on the machine where it matters.
#
# Exit 1 on any violation, naming the pack file, the rule id, and the exact sentence to delete.
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v node >/dev/null 2>&1; then
  echo "check-pack-suppress-sentence: \`node\` not found. This guard parses pack JSON rather than" >&2
  echo "  scanning it as text (pack files are mostly regex source — a substring scan for \"message\"" >&2
  echo "  misreads quoting), so it cannot degrade to grep. Install node; skipping would report clean" >&2
  echo "  while checking nothing." >&2
  exit 1
fi

node --input-type=module -e '
import fs from "fs";
import path from "path";

const root = "rules/dsl";
// Enumerated from the directory the engine itself loads, never a hand-typed pack list: a new pack is
// exactly the pack nobody has reviewed, and a hand list stops covering it the day it lands.
const packFiles = [];
for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
  if (!entry.isDirectory()) continue;
  const dir = path.join(root, entry.name);
  for (const f of fs.readdirSync(dir)) {
    if (f.endsWith(".json")) packFiles.push(path.join(dir, f));
  }
}
packFiles.sort();

if (packFiles.length === 0) {
  console.error("check-pack-suppress-sentence: enumerated ZERO pack files under " + root + " — the");
  console.error("  scan root is wrong or the packs are gone. An empty subject set is a broken guard,");
  console.error("  never a clean tree.");
  process.exit(1);
}

// The two sentences `zzop_core::dsl::suppress_hint` can produce, by matcher kind. Kept in step with
// that function BY THIS GUARD FAILING if it drifts: the tail it looks for is the tail the engine
// writes, so a reworded append makes every hand-written copy invisible here — which is why
// `crates/core/src/dsl/markers/channel.rs` carries its own byte pin on the same strings
// (`the_per_file_sentence_is_byte_identical_to_the_one_the_packs_used_to_carry`). Two pins, because
// one of them alone can go stale silently.
const sentenceFor = (kind, marker) => {
  if (kind === "line-scan" || kind === "method-scan") {
    return "Suppress a vetted case with `// " + marker + "`.";
  }
  if (kind === "call-scan" || kind === "literal-scan") {
    return "Suppress a vetted case with `// " + marker + "` (`# " + marker + "` in Python).";
  }
  // symbol-scan (no anchor line) and io-scan (anchor line re-read through a callback envelope mode
  // answers with None) get NO appended sentence, so anything their authors write is the only copy.
  return null;
};

const offenders = [];
let rulesJudged = 0;

for (const file of packFiles) {
  const pack = JSON.parse(fs.readFileSync(file, "utf8"));
  for (const rule of pack.rules || []) {
    const kind = rule.matcher && rule.matcher.type;
    const marker = "zzop-" + rule.id + "-ok";
    const sentence = sentenceFor(kind, marker);
    if (sentence === null) continue;
    rulesJudged++;

    const message = rule.message || "";
    if (!message.endsWith(sentence)) continue;

    // Simulate the append on the message WITHOUT this tail. If `suppress_hint`s opt-out would fire
    // (the marker is still named elsewhere), the engine adds nothing and this copy is the only one
    // the reader gets — legitimate, keep it.
    const withoutTail = message.slice(0, message.length - sentence.length).trimEnd();
    if (withoutTail.includes(marker)) continue;

    offenders.push(
      "  " + file + " — rule `" + pack.id + "/" + rule.id + "` (" + kind + ")\n" +
      "    delete this trailing sentence: " + JSON.stringify(sentence)
    );
  }
}

if (rulesJudged === 0) {
  console.error("check-pack-suppress-sentence: parsed " + packFiles.length + " pack file(s) but judged");
  console.error("  ZERO rules — every matcher kind read as one that gets no appended sentence, which");
  console.error("  cannot be true while this repo ships line-scan rules. The matcher-kind read is");
  console.error("  broken and a green here would mean nothing.");
  process.exit(1);
}

if (offenders.length > 0) {
  console.error("check-pack-suppress-sentence: DSL rule messages that hand-write the engine-appended");
  console.error("suppress sentence:");
  console.error("");
  console.error(offenders.join("\n"));
  console.error("");
  console.error("The engine appends this sentence to every one of these rules findings already");
  console.error("(zzop_core::dsl::suppress_hint, via zzop-engine pipeline::findings::append_hints), so");
  console.error("writing it in `message` makes the finding a user reads carry it TWICE. Delete the");
  console.error("trailing sentence and write only what the append cannot know — see");
  console.error("docs/rules/authoring-guide.md.");
  console.error("");
  console.error("If your rule needs DIFFERENT wording (a `#` leader for a config-file rule, a carve-out,");
  console.error("an envelope caveat), keep your sentence and name the marker once more elsewhere in the");
  console.error("message — the append opts out when the message already names it.");
  process.exit(1);
}

console.log(
  "check-pack-suppress-sentence: clean (" + rulesJudged + " marker-bearing rules across " +
  packFiles.length + " packs; 0 hand-written copies of the engine-appended sentence)."
);
'
