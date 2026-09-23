#!/usr/bin/env bash
# check-mcp-tools-listed.sh — every tool the MCP server defines must be named in the shipped prose
# that claims to list them, and that prose must name no tool the server does not define.
#
# ## The defect this exists for (2026-09-06, external review lane A)
#
# `README.md` said "The server exposes the tools ..." and named SEVEN. The server defines EIGHT;
# `check_coverage` was missing, and had been for as long as anyone measured. Nothing caught it:
# `check-embedded-contract-docs.sh` guards the contract RESOURCES, `check-rules-catalog-sync.sh` guards
# rule ids, and the tool roster sat between them with no owner.
#
# The cost is specific rather than cosmetic. README is the install page an agent's operator reads to
# decide what zzop can answer; a capability that ships and is never named is one nobody asks for, and
# it is the same silent-failure shape this repo hunts everywhere else — the tool works, the run is
# green, and the sentence is quietly short.
#
# ## Why the SSOT is the definitions file and not the running server
#
# Asking the binary would mean building it: a 30s+ release build inside a guard fleet that is expected
# to finish in seconds, for an answer that is a literal in a tracked file. `definitions.rs` is where a
# tool is born — a tool cannot reach the wire without a `"name"` here — so a text scan of it is not an
# approximation of the roster, it IS the roster. The cost of that choice is stated rather than hidden:
# this guard cannot see a tool that is registered somewhere else, and `check-guards-wired.sh` is the
# thing that would notice if a second registration path appeared.
#
# ## Both directions, on purpose
#
# A missing tool is the defect that shipped. An EXTRA tool in prose — a name that was renamed or
# removed and left behind in the sentence — is the same defect pointing the other way, and the more
# expensive one: it sends a reader to call something that answers `-32601`.
#
# ## The second page, and why this guard missed it for six days (2026-09-12, review ledger V168)
#
# This guard's headline says "the shipped prose that claims to list them" — plural — and its
# implementation read ONE page. `docs/modules/mcp.md` carries a `### Tools` TABLE that also claims to
# list them, and `check_coverage` was missing from it too: the 2026-09-06 repair fixed README, this
# guard was written to hold README, and the sibling page kept the same hole while the guard reported
# OK. Worse, that page's own prose 130 lines up ANNOUNCED the promotion, so the file disagreed with
# itself and nothing read both halves.
#
# The lesson is the guard's own header sentence read back: "the tool roster sat between them with no
# owner" was true of a SET of pages, and taking ownership of one member of that set is what left the
# rest looking owned. Subjects are a list here now; adding a third page is adding a row.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

DEFS=packages/mcp/src/tools/definitions.rs
PROSE=README.md
TABLE_PROSE=docs/modules/mcp.md
# The sentence, not the file: README names tools in other places (a `check_file` example, a recipe),
# and holding the whole file to the roster would demand every mention be exhaustive. The anchor is the
# claim itself -- the line that says it is listing them.
ANCHOR='The server exposes the tools'

for f in "$DEFS" "$PROSE" "$TABLE_PROSE"; do
  [ -f "$f" ] || { echo "check-mcp-tools-listed: missing $f -- re-anchor this guard." >&2; exit 1; }
done

set +e
tools="$(grep -oP '"name":\s*"\K[a-z_]+' "$DEFS" | sort -u)"
rc=$?
set -e
if [ "$rc" -ne 0 ] || [ -z "$tools" ]; then
  echo "check-mcp-tools-listed: extracted ZERO tool names from $DEFS (grep exit $rc)." >&2
  echo "  That is a broken needle, never a server with no tools. Re-anchor this guard." >&2
  exit 1
fi
n_tools="$(printf '%s\n' "$tools" | awk 'END { print NR }')"

# The claim sentence plus the two lines after it: the roster wraps, and a guard that read one line
# would fail on formatting rather than on truth.
set +e
claim="$(grep -A 2 -F "$ANCHOR" "$PROSE")"
rc=$?
set -e
if [ "$rc" -ne 0 ] || [ -z "$claim" ]; then
  echo "check-mcp-tools-listed: '$ANCHOR' not found in $PROSE." >&2
  echo "  Either the sentence was reworded (re-anchor here) or the roster claim was deleted, which" >&2
  echo "  would leave $n_tools shipped tools with no page naming them." >&2
  exit 1
fi

missing=""
for t in $tools; do
  case "$claim" in
    *"\`$t\`"*) ;;
    *) missing="$missing $t" ;;
  esac
done

# The other direction: a backticked `snake_case` token inside the claim that no longer names a tool.
set +e
claimed="$(printf '%s' "$claim" | grep -oP '`\K[a-z]+_[a-z_]+(?=`)' | sort -u)"
set -e
extra=""
for c in $claimed; do
  case " $(printf '%s' "$tools" | tr '\n' ' ') " in
    *" $c "*) ;;
    *) extra="$extra $c" ;;
  esac
done

if [ -n "$missing" ] || [ -n "$extra" ]; then
  echo "check-mcp-tools-listed: $PROSE's tool roster disagrees with $DEFS." >&2
  [ -n "$missing" ] && echo "  SHIPPED BUT UNNAMED:$missing -- a capability nobody will ask for." >&2
  [ -n "$extra" ] && echo "  NAMED BUT NOT SHIPPED:$extra -- a reader who calls it gets -32601." >&2
  echo "  Server defines $n_tools tool(s): $(printf '%s' "$tools" | tr '\n' ' ')" >&2
  exit 1
fi

# --- Subject 2: the `### Tools` TABLE in docs/modules/mcp.md -------------------------------------
# Only the FIRST CELL of each row is the roster. The Purpose cells legitimately name other tools
# ("ask analyze_repo/cross_repo for those"), and holding those to the roster would make a cross
# reference a violation -- the same over-reach the README anchor comment already rejects.
set +e
rows="$(grep -oP '^\| `\K[a-z_]+(?=` \|)' "$TABLE_PROSE" | sort -u)"
set -e
if [ -z "$rows" ]; then
  echo "check-mcp-tools-listed: extracted ZERO tool rows from $TABLE_PROSE's table." >&2
  echo "  That is a broken needle or a deleted table, never a server with no tools. Re-anchor." >&2
  exit 1
fi

t_missing=""
for t in $tools; do
  case " $(printf '%s' "$rows" | tr '\n' ' ') " in
    *" $t "*) ;;
    *) t_missing="$t_missing $t" ;;
  esac
done
t_extra=""
for c in $rows; do
  case " $(printf '%s' "$tools" | tr '\n' ' ') " in
    *" $c "*) ;;
    *) t_extra="$t_extra $c" ;;
  esac
done

if [ -n "$t_missing" ] || [ -n "$t_extra" ]; then
  echo "check-mcp-tools-listed: $TABLE_PROSE's tool TABLE disagrees with $DEFS." >&2
  [ -n "$t_missing" ] && echo "  SHIPPED BUT UNLISTED:$t_missing -- a capability nobody will ask for." >&2
  [ -n "$t_extra" ] && echo "  LISTED BUT NOT SHIPPED:$t_extra -- a reader who calls it gets -32601." >&2
  echo "  Server defines $n_tools tool(s): $(printf '%s' "$tools" | tr '\n' ' ')" >&2
  exit 1
fi

n_rows="$(printf '%s\n' "$rows" | awk 'END { print NR }')"
echo "check-mcp-tools-listed: OK ($n_tools tools defined in $DEFS; all named in $PROSE's roster and all listed in $TABLE_PROSE's table ($n_rows rows); none extra on either page)."
