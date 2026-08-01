#!/usr/bin/env bash
# Guard: the retired "zzop runs without a config" claim must not come back to a surface a user reads.
#
# ## Why this exists
# On 2026-07-27 a config file became mandatory for every analysis lane, reversing the founding default.
# The behaviour changed in one commit; the SENTENCES describing it did not. A 2026-08-01 domain review
# found the old claim still shipping in the MCP tool descriptions, the CLI help, four `crates/summary`
# doc comments and three `docs/modules/mcp.md` paragraphs — and the worst copy said the exact opposite
# of the code: that a `zzop.config.jsonc` inside a `paths`-mode root "is NOT loaded". `zzop_config`'s
# `trees.rs` loads it and errors when it is missing, and `crates/summary/tests/host_dispatch.rs` pins
# the reply's own disclosure ("paths mode loaded each tree's own ..."). An agent reading the tool
# description would conclude the inverse of what the tool does.
#
# ## Why PHRASES and not the bare token
# The first cut of this guard banned `zero-config` outright and immediately proved why that is wrong:
# the token carries three different meanings in this repo, and only one is dead.
#
#   dead   "a zero-config run", "config-free paths mode"  -> no such thing since 2026-07-27
#   alive  "zero-config envelope analysis"                -> Mode A carries no filesystem location at
#                                                            all, so it genuinely takes no config
#   alive  "zzop's value is zero-config coverage"         -> a comparison to tools that need a config
#                                                            of their own (ESLint); nothing to do with
#                                                            zzop's own config file
#
# A guard that cannot tell those apart fires on correct sentences, and a warning that is usually noise
# is a warning nobody reads. So the subject is an explicit list of DEAD PHRASES: each one is checked
# here because it has no true present-tense reading, and adding to the list is a deliberate act.
#
# Anchors and URL fragments (`id="zero-config"`, `href="#zero-config"`) are deliberately NOT matched: a
# fragment is an identifier, not a claim, and renaming one breaks inbound links for no honesty gain.
set -euo pipefail
cd "$(dirname "$0")/.."

scan_paths=(packages docs site README.md crates)

# Each entry has no true present-tense reading. Keep them lowercase; the grep is case-insensitive.
dead_phrases=(
  'config-free'
  'zero-config run'
  'zero-config paths'
  'zero-config default'
  'zero-config = full analysis'
  'zero config run'
)

# A reversal marker makes a line a HISTORICAL record — the way this repo documents that it stopped
# doing something, which is the opposite of the defect and must stay writable.
historical='reversing|used to|Before this|no longer|there is no zero-config|20[0-9][0-9]-[0-9][0-9]-[0-9][0-9]'

pattern="$(IFS='|'; echo "${dead_phrases[*]}")"

offenders=""
while IFS= read -r hit; do
  [ -n "$hit" ] || continue
  text="${hit#*:}"; text="${text#*:}"
  # A URL fragment or an HTML id is an identifier, not a sentence. Herestrings rather than pipes:
  # `| grep -q` under pipefail SIGPIPEs the producer and can flip this loop's verdict (the repo has a
  # guard for exactly that, and it caught this script on its first run).
  grep -Eq 'id="[^"]*zero-config|href="#[^"]*zero-config' <<< "$text" && continue
  grep -Eiq "$historical" <<< "$text" && continue
  offenders="${offenders}${hit}"$'\n'
done < <(grep -rni -E "$pattern" "${scan_paths[@]}" 2>/dev/null || true)

# Non-emptiness floor: if the scan roots move or the extraction breaks, an empty result is
# indistinguishable from a clean tree and this guard would pass forever by reading nothing.
scanned="$(grep -rl -E 'zzop\.config\.jsonc' "${scan_paths[@]}" 2>/dev/null | wc -l | tr -d ' ')"
if [ "$scanned" -lt 20 ]; then
  echo "check-retired-config-vocabulary: only $scanned file(s) mention zzop.config.jsonc — the scan" >&2
  echo "  roots are wrong and this guard would be vacuously green. Fix the roots, not this number." >&2
  exit 1
fi

if [ -n "$offenders" ]; then
  echo "check-retired-config-vocabulary: a RETIRED claim is live on a user-facing surface:" >&2
  printf '%s' "$offenders" | sed 's/^/  /' >&2
  echo >&2
  echo "A config file has been mandatory for every analysis lane since 2026-07-27. In paths mode each" >&2
  echo "root LOADS its own zzop.config.jsonc (zzop_config::trees) and the reply discloses the honored" >&2
  echo "files in configWarnings while \`config\` stays null." >&2
  echo >&2
  echo "Say what happens now. If the line is a HISTORICAL record, keep a reversal marker on it" >&2
  echo "('reversing', 'used to', 'Before this', or the date). If you mean the ENVELOPE lane or a" >&2
  echo "comparison to other tools, word it so — those senses are alive and this guard does not match" >&2
  echo "them (see the phrase table in this script's header)." >&2
  exit 1
fi

echo "check-retired-config-vocabulary: clean ($scanned file(s) mentioning zzop.config.jsonc scanned)."
