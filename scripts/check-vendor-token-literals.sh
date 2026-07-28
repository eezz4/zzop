#!/usr/bin/env bash
# vendor-token literal guard — fails when a TRACKED file contains a contiguous, well-formed vendor
# credential literal (Slack / GitHub / Stripe / AWS / Google).
#
# ## Why this exists (bit for real 2026-07-25, cost: a blocked release)
# GitHub push protection rejected BOTH the v0.24.0 main push and the tag push over
# `rules/dsl/security/secrets_vetoes.rs:277`, which held a Slack-token-shaped literal. The value was
# synthetic and a comment directly above it said so — the scanner does not read comments. The repo
# already had the right convention: `rules/dsl/security/vendor_token_committed.rs`'s header splits
# every such fixture as `concat!("xo", "xb-...")`, so the reassembled token still reaches the rule at
# runtime while no contiguous prefix ever appears in the raw source. That convention lived ONLY in a
# comment, so it stopped nothing — this script is the mechanical half.
#
# ## Why it must run at COMMIT time
# Push protection fires at push time, which is far too late: by then the release is already cut and
# the tag push fails too. So this is wired into `.githooks/pre-commit` (plus CI, per
# check-guards-wired.sh's both-locations requirement) — the commit that would create the unpushable
# history is the one that gets blocked.
#
# ## What is scanned: the INDEX first, then the working tree (both fail)
# This guard scans BOTH surfaces, and that is the one place it deliberately breaks the repo-wide
# working-tree convention .githooks/pre-commit otherwise states.
#
# Bit for real 2026-07-26, one commit after this guard was written: the first version enumerated
# paths from `git ls-files` (the INDEX) but then grepped the file ON DISK. Those are different
# contents, and the mismatch is a false GREEN in the exact direction that reproduces the original
# incident — reproduced verbatim:
#     printf 'let s = "<contiguous token>";\n' > staged.rs && git add staged.rs   # index HAS it
#     printf 'let s = "clean";\n'              > staged.rs                        # worktree does not
#     bash scripts/check-vendor-token-literals.sh   -> exit 0, "clean"
#     git commit && git show HEAD:staged.rs         -> the token is in the commit, push is rejected
# GitHub scans the BLOB a push carries, and the blob a commit carries is the INDEX's content, never
# the working tree's. So the index is the authoritative surface here.
#
# The index surface alone is sufficient for correctness (nothing can reach a push without passing
# through the index), so the working-tree pass is a deliberate addition, not redundancy: it is what
# the next `git add -A` will stage, and catching it one commit earlier costs nothing when the remedy
# is a one-line `concat!` split that is required anyway. Its accepted cost is a token parked unstaged
# in a tracked file blocking an unrelated commit — accepted because, per "No escape hatch" below,
# there is no legitimate form for a contiguous vendor token in a tracked file on either surface.
#
# Both passes use `git grep`, which reads the index directly (`--cached`) or the tracked-path set
# against the working tree (no flag) — so `$GIT_INDEX_FILE` is honored, which matters: `git commit -a`
# and `git commit <paths>` build a TEMPORARY index and run the pre-commit hook against it. A scan
# that read `.git/index` by hand would judge the wrong bytes for both of those. Tracked paths missing
# from the working tree (a normal transient mid-rename) are skipped by `git grep` silently, with
# nothing on stderr.
#
# ## Detection shape: prefix PLUS a body, never a bare prefix
# The needles below all require a token BODY after the vendor prefix. That is deliberate and load
# bearing, not laziness: a bare `sk_live_` / `ghp_` / `xoxb-` mention is ordinary prose, and this repo
# is a secret SCANNER, so those prefixes legitimately appear in rule messages, the rule regexes
# themselves (`\bghp_[A-Za-z0-9]{36}\b` in rules/dsl/security/security.json), the generated
# docs/rules/catalog.md, and site/rules.html. Push protection does not block any of those, and a guard
# that did would either be silenced or would force splitting text that must stay byte-identical to the
# rule JSON it is generated from.
#
# Re-measured 2026-07-26 against the whole tracked tree, because the numbers this paragraph used to
# carry ("matches exactly the five real fixture literals") were taken BEFORE the `concat!` splits that
# this guard's introduction forced, and were false the moment those splits landed:
#   * body-requiring needles, working tree: 0 lines. Every fixture is now split; the steady state of a
#     tree that satisfies this guard is zero, and any nonzero is the violation.
#   * body-requiring needles, HEAD's stored blobs at the time of writing: 6 lines across 2 files
#     (rules/dsl/security/secrets.rs, rules/dsl/security/secrets_vetoes.rs) — the pre-split state, i.e.
#     exactly what the index pass above exists to see and the working-tree pass structurally cannot.
#   * bare-prefix mentions the needles deliberately do NOT match: 13 lines across 5 tracked files
#     (docs/rules/catalog.md, rules/dsl/security/secrets.rs, rules/dsl/security/security.json,
#     rules/dsl/security/vendor_token_committed.rs, site/rules.html). Those 13 are the false-red budget
#     the body requirement buys back.
#
# ## Which vendors, and what is deliberately absent
# The needle set is "shapes GitHub push protection actually blocks that a SAST fixture tree plausibly
# grows", swept across the whole tracked tree at introduction against a much wider candidate list
# (GitLab `glpat-`, fine-grained `github_pat_`, npm, SendGrid, Slack app-level `xapp-`, Google `ya29.`,
# Square, DigitalOcean, Shopify, Twilio, the other AWS key-id prefixes). Only one shape outside the
# originally-specified list was actually present -- a Google OAuth client secret (`GOCSPX-`) in
# rules/dsl/security/secrets.rs -- so it is in the list and that literal is now split. The rest are
# left out rather than added speculatively: each needle is a standing false-red risk, and an unmatched
# needle buys nothing until a fixture in that shape exists.
# PEM private keys are the one knowing omission. rules/dsl/security/private_key_committed.rs carries
# seven `-----BEGIN ... PRIVATE KEY-----` fixtures, but every body is a truncated `MIIEow...` stub, not
# parseable key material, so push protection does not block them and a needle here would only false-red
# on legitimate fixtures. Revisit if a fixture ever needs a full key body.
#
# ## No escape hatch, on purpose
# Every other guard here has one. This one does not: a contiguous full token in tracked source has no
# legitimate form, and the remedy (a compile-time split) costs one line and changes no behavior. Known
# limitation of that stance: a NON-Rust tracked file needing a full token shape would have no `concat!`
# to reach for. No such file exists today; if one ever does, the fix is to keep the fixture out of the
# tracked tree (a temp-dir write at test runtime), not to widen this guard.
#
# ## Self-consistency
# This script passes its own check. Every needle is written so no contiguous body-bearing token exists
# in it either — a character class (`xox[bpars]-`) or a regex bracket immediately after the prefix
# (`ghp_[A-Za-z0-9]...`) breaks contiguity for free. The failure output prints file:line and the token
# KIND only, never the matched text: `cut -d: -f1,2` can never reach `git grep -n`'s text field, which
# is always field 3 or later regardless of how many colons a path contains, so a real leak is not
# re-emitted into CI logs.
#
# ## Coverage census (why the OK line carries a number)
# The success line prints how many tracked paths were in scope. A guard whose enumeration silently
# collapses to the empty set exits 0 and reads identically to a guard that swept the tree — this repo
# has already shipped that failure in another form (22 zero-byte artifacts read as total success). The
# count is also ASSERTED nonzero below, so "scanned nothing" is a failure rather than a pass.
#
# Scope: TRACKED paths only, on both surfaces. That is the exact set a push carries, and it excludes
# the gitignored `/corpus/oss` and `/scratchpad`, plus `target/` and `node_modules/`, by construction,
# with no path list of its own to drift. The 2026-07-26 promotion of the labeled benchmark from
# `corpus/benchmark/` to the tracked `cases/` is the first case where that exclusion carried
# weight: it produced exactly the non-Rust-tracked-file scenario "No escape hatch" above anticipated,
# and was resolved the way that paragraph prescribes — the one offending fixture is named individually
# in .gitignore (with the consequence written down there), not exempted here. (This is also why this
# guard does NOT take the --others
# --exclude-standard widening check-english-source.sh takes: that guard's subject is a file's CONTENT
# LANGUAGE, which someone may want flagged before staging; this guard's subject is a push-blocking
# property of committed blobs, and an unstaged untracked file has no blob.)
#
# No deps beyond git — the needles are POSIX ERE, so no `grep -P`/PCRE build is required of git or of
# the host. Exit 1 on any violation.
set -euo pipefail
cd "$(dirname "$0")/.."

# "<kind>|<ERE>" per entry. Body lengths sit BELOW each vendor's real token length, so a
# truncated-but-still-well-formed placeholder is caught too, and far above anything prose produces.
# The prefixes appear here as bare prefixes only (each is immediately followed by a regex bracket, or
# is itself a character class), which is why this file does not match its own needles.
KINDS=(
  "Slack token (xox[bpars]- family)|xox[bpars]-[A-Za-z0-9-]{10,}"
  "GitHub personal access token (ghp_ prefix)|ghp_[A-Za-z0-9]{20,}"
  "GitHub OAuth token (gho_ prefix)|gho_[A-Za-z0-9]{20,}"
  "Stripe live secret key (sk_live_ prefix)|sk_live_[A-Za-z0-9]{10,}"
  "Stripe live restricted key (rk_live_ prefix)|rk_live_[A-Za-z0-9]{10,}"
  "AWS access key id (AKIA/ASIA prefix)|(AKIA|ASIA)[A-Z0-9]{16}"
  "Google API key (AIza prefix)|AIza[A-Za-z0-9_-]{20,}"
  "Google OAuth client secret (GOCSPX- prefix)|GOCSPX-[A-Za-z0-9_-]{15,}"
)

# One alternation for the cheap first pass over each surface.
combined=""
for entry in "${KINDS[@]}"; do
  [ -n "$combined" ] && combined="$combined|"
  combined="$combined${entry#*|}"
done

# gg <surface> <outfile> <git-grep-args...> -> writes matches to <outfile>; returns 0 whether or not
# anything matched, and ABORTS THE SCRIPT on a real git failure. `git grep` exits 1 for "no match" and
# >= 2 for a real error, and the two must not share a verdict — the `$(... || true)` idiom this repo
# has been bitten by before swallows both, letting a producer that never ran read as "clean". Written
# to a file rather than captured in `$(...)` so the abort is a real script exit, not a subshell exit
# whose only remaining signal is an exit status somebody may later wrap.
# `-a` (treat binaries as text) is deliberate: push protection reads binary blobs too, and `-n` line
# numbers stay meaningful under it.
gg() {
  local surface="$1" out="$2"; shift 2
  local err rc
  err="$(mktemp)"
  set +e
  git grep "$@" > "$out" 2> "$err"
  rc=$?
  set -e
  if [ "$rc" -gt 1 ]; then
    echo "check-vendor-token-literals: 'git grep' failed on the $surface surface (exit $rc) --" >&2
    echo "  aborting rather than report a clean tree from a scan that never ran:" >&2
    cat "$err" >&2
    rm -f "$err" "$out"
    exit 1
  fi
  rm -f "$err"
}

# `.` as the pathspec = every tracked path under the repo root.
fail=0
scratch="$(mktemp)"
trap 'rm -f "$scratch"' EXIT
for surface in index worktree; do
  # Never an EMPTY array here (`-anE` always rides along): `"${empty[@]}"` under `set -u` is an
  # unbound-variable error on bash < 4.4, and this script must not depend on the host's bash minor.
  if [ "$surface" = index ]; then
    base=(--cached -anE)
    label="INDEX (staged content — what the commit will carry)"
  else
    base=(-anE)
    label="WORKING TREE"
  fi

  gg "$surface" "$scratch" "${base[@]}" -e "$combined" -- .
  [ -s "$scratch" ] || continue

  # Something is wrong on this surface; re-scan per kind to attribute each line, paying the extra
  # process spawns only on the failure path.
  surface_fail=0
  for entry in "${KINDS[@]}"; do
    kind="${entry%%|*}"
    re="${entry#*|}"
    gg "$surface" "$scratch" "${base[@]}" -e "$re" -- .
    # file:line only — `git grep -n` prints `<path>:<line>:<text>`, and the text can never survive
    # `cut -d: -f1,2` (see "Self-consistency" above).
    locs="$(cut -d: -f1,2 < "$scratch")"
    [ -n "$locs" ] || continue
    while IFS= read -r loc; do
      [ -n "$loc" ] || continue
      echo "check-vendor-token-literals: [$label] $loc -- $kind" >&2
      surface_fail=1
    done <<< "$locs"
  done
  # A combined-pass hit that no per-kind pass reproduces would mean the two spellings disagree.
  # Report it rather than let the surface fall through as clean.
  if [ "$surface_fail" -eq 0 ]; then
    echo "check-vendor-token-literals: [$label] the combined needle matched but no individual kind did" >&2
    echo "  -- the alternation and the per-kind list have diverged. Treating as a violation." >&2
    surface_fail=1
  fi
  fail=1
done

if [ "$fail" -ne 0 ]; then
  echo >&2
  echo "check-vendor-token-literals: FAILED -- a contiguous vendor-token literal is in tracked source." >&2
  echo "  GitHub push protection scans RAW SOURCE TEXT and does not read the comment next to it saying" >&2
  echo "  the value is synthetic. Left in, it blocks the branch push AND the tag push of the next release." >&2
  echo "  An [INDEX] hit is content already staged: it is what the next commit carries, so fixing the" >&2
  echo "  working-tree copy is not enough -- 'git add' the fix and re-run." >&2
  echo "  Fix: split the literal so no contiguous prefix+body survives in the file, while the value the" >&2
  echo "  test actually sees is unchanged. In Rust that is concat!, joined at compile time:" >&2
  echo "      const SLACK_BOT: &str = concat!(\"xo\", \"xb-1234567890-abcdEFGH1234\");" >&2
  echo "  See rules/dsl/security/vendor_token_committed.rs's header for the established convention and" >&2
  echo "  every already-split fixture. There is deliberately no escape-hatch marker for this guard." >&2
  exit 1
fi

# Coverage census + its assertion (see "Coverage census" in the header).
scanned="$(git ls-files -- . | wc -l | tr -d '[:space:]')"
if [ "$scanned" -eq 0 ]; then
  echo "check-vendor-token-literals: enumerated 0 tracked paths -- the scan swept nothing, so its" >&2
  echo "  'clean' verdict is vacuous. Something is wrong with the repository or the pathspec." >&2
  exit 1
fi
echo "check-vendor-token-literals: clean ($scanned tracked paths scanned on both the index and the working tree; no contiguous vendor-token literals)."
