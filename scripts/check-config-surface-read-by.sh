#!/usr/bin/env bash
# check-config-surface-read-by.sh — `vocabularyReadBy` is DERIVED, so it must never be edited by hand.
#
# ## What it answers, and why it needed a guard on day one
# 42 convention-vocabulary keys ship in the starter config and the key NAME says the language for only
# some of them. A Python-only project receives `retryWrappers` prefilled with four JavaScript library
# names and nothing tells it that no Python lane reads that key (external review round 22, ledger
# V241). `configKeys.vocabulary` is a flat list of names; `vocabularyReadBy` is the column that answers
# it, one entry per key.
#
# ## The derivation, stated so a reader can disagree with it
# A vocabulary field is read by a language's extractor iff its snake_case field name appears in that
# language's `parser/parser-<lang>` crate. Otherwise the reader is looked up rather than assumed:
# `config-frontend` when only crates/config mentions it, `engine` when the analysis layers do. That is
# a MEASUREMENT of who reads the name, not a claim about which languages the key ultimately affects --
# `_docs.vocabularyReadBy` carries that limit, with `javaSourceRoot` as the worked example (name says
# Java, reader is a native analysis in crates/engine).
#
# The third answer arrived on 2026-09-15 (ledger V250). Until then the else-branch was a bare
# `["engine"]`, so a key the FRONT END consumes and never forwards was labelled as read by the engine
# -- which for `workspaceSkipDirs` was false, its only crates/engine mention being a rustdoc link to a
# field that does not exist. A default bucket that cannot be wrong is a bucket that says nothing, and
# 34 of 42 keys were in it.
#
# ## Why a guard rather than a comment saying "regenerate this"
# A derived table that a person can edit is a hand-maintained table with extra steps, and this repo has
# measured the same decay in four other places. `--update` rewrites it; without the flag this FAILS when
# the committed table differs from what the tree derives, so a new vocabulary key cannot land with no
# entry and a renamed parser crate cannot silently move every key to `engine`.
set -euo pipefail
cd "$(dirname "$0")/.."

SURFACE="crates/config/config-surface.json"
derived="$(python3 - "$SURFACE" <<'PY'
import json, re, os, subprocess, sys, collections
surface = json.load(open(sys.argv[1], encoding="utf-8"))
keys = sorted(surface["configKeys"]["vocabulary"])
snake = lambda n: re.sub(r"(?<!^)(?=[A-Z])", "_", n).lower()
lanes = sorted(d for d in os.listdir("parser") if d.startswith("parser-"))
out = collections.OrderedDict()
for k in keys:
    f = snake(k)
    hits = [lane.replace("parser-", "") for lane in lanes
            if subprocess.run(["grep", "-rl", "--include=*.rs", f, f"parser/{lane}"],
                              capture_output=True, text=True).stdout.strip()]
    if hits:
        out[k] = hits
        continue
    # NOT a parser field. Two very different things used to land in one bucket called `engine`
    # (external review round 23, ledger V250): a key the ENGINE reads, and a key the CONFIG FRONT END
    # consumes and never forwards. `workspaceSkipDirs` is the second, and calling it `engine` was
    # simply false -- its only hit under crates/engine is a rustdoc link to a field that does not
    # exist. A default bucket that cannot be wrong is a bucket that says nothing.
    def reads(*dirs):
        # CODE lines only. A mention inside a `//` or `///` line is prose, not a read -- and the case
        # that forced this is exactly that: `workspaceSkipDirs`'s only crates/engine hit is a rustdoc
        # intra-doc link to a field that does not exist, which used to count as "the engine reads it".
        r = subprocess.run(["grep", "-rn", "--include=*.rs", f, *dirs],
                           capture_output=True, text=True)
        for line in r.stdout.split("\n"):
            body = line.split(":", 2)[-1].lstrip()
            if body and not body.startswith("//"):
                return True
        return False

    front_end = reads("crates/config/src")
    engine = reads("crates/engine/src", "crates/metrics/src", "crates/facade/src",
                   "crates/core/src", "rules")
    if front_end and not engine:
        out[k] = ["config-frontend"]
    else:
        out[k] = ["engine"]
print(json.dumps(out, indent=2, ensure_ascii=False))
PY
)"

committed="$(python3 -c '
import json, sys, collections
j = json.load(open(sys.argv[1], encoding="utf-8"), object_pairs_hook=collections.OrderedDict)
print(json.dumps(j.get("vocabularyReadBy", {}), indent=2, ensure_ascii=False))
' "$SURFACE")"

if [ "${1:-}" = "--update" ]; then
  python3 - "$SURFACE" "$derived" <<'PY'
import json, sys, collections, io
p = sys.argv[1]
j = json.load(io.open(p, encoding="utf-8"), object_pairs_hook=collections.OrderedDict)
j["vocabularyReadBy"] = json.loads(sys.argv[2], object_pairs_hook=collections.OrderedDict)
io.open(p, "w", encoding="utf-8").write(json.dumps(j, indent=2, ensure_ascii=False) + "\n")
PY
  echo "check-config-surface-read-by: rewrote vocabularyReadBy in $SURFACE"
  exit 0
fi

if [ "$derived" != "$committed" ]; then
  echo "check-config-surface-read-by: vocabularyReadBy is DERIVED and has drifted from the tree."
  diff <(printf '%s\n' "$committed") <(printf '%s\n' "$derived") | head -40 || true
  echo "  A vocabulary key with no entry, or an entry no longer true, leaves a config author with no way"
  echo "  to tell whether a key does anything for their stack — which is the gap this column closes."
  echo "  Regenerate: bash scripts/check-config-surface-read-by.sh --update"
  exit 1
fi

n=$(printf '%s\n' "$derived" | grep -c '^  "')
echo "check-config-surface-read-by: OK ($n vocabulary key(s), each with a measured reader)."
