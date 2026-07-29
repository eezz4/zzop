#!/usr/bin/env bash
# Docs link-graph guard — fails when a documentation page or example is orphaned from its hub.
#
# The drift class (bitten 2026-07-16): docs/modules/mcp.md shipped in v0.16.0 but the docs hub
# (docs/README.md) never gained a row, and examples/README.md's index listed only 6 of 9 entries
# (auth-overlay-adapter, adapter-kit missing) — new pages were un-discoverable
# from the surface readers actually start at.
#
# Two containment checks (asymmetric, token-level — same idiom as the other sync guards):
#   1. every tracked docs/**/*.md (except the hub itself) must be referenced by its docs-relative
#      path (e.g. `modules/mcp.md`, `adapters/README.md`) somewhere in docs/README.md.
#   2. every entry directly under an examples hub — examples/ and examples/adapters/ (directory or
#      file, except that hub's own README.md) — must be referenced by name somewhere in that hub's
#      README.md.
# Hub prose quality is NOT checked — only that a reference exists at all.
#
# Check 3 (added 2026-07-25) is the other half: reachability is worth nothing if the link that
# reaches you is broken. Checks 1-2 only ask whether a hub CONTAINS a page's path as a substring —
# they never resolve a single link target, so `[x](../README.md)` from a two-deep directory, or a
# `#section` that was renamed out from under its referrer, both read green. Check 3 resolves every
# local Markdown link in every tracked *.md: the target file must exist, and a `#fragment` must
# match a heading on that page under GitHub anchor slugging (lowercase, drop everything outside
# [a-z0-9 _-], spaces to hyphens; repeats take GitHub's -1/-2 suffix). Fenced code blocks are
# skipped so a `# comment` inside one is not mistaken for a heading. Measured at introduction:
# 68 fragment links against 216 heading slugs, zero dead anchors, exactly one dead file link
# (packages/mcpb/README.md pointing at a packages/README.md that never existed) — fixed, not
# baselined. Scope is deliberately every tracked *.md, not just docs/: that one real defect lived
# outside docs/, and no other guard looks at packages/ prose.
#
# No deps beyond git + grep + awk. Exit 1 on any orphan, listing them.
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0

# --- Check 1: docs/**/*.md all referenced from docs/README.md ---
# The hub is slurped ONCE and searched with bash's own pattern match, not re-read by a `grep -qF` per
# page (2026-07-29). "Is this fixed string somewhere in the hub" is exactly what `[[ $s == *"$x"* ]]`
# answers — same substring semantics as `grep -F`, no anchoring on either — and the per-page grep was
# one process per docs page. On this box that is the guard's whole cost: its `sys` time dwarfs its
# `user` time. `$(< file)` is bash's no-fork file read (it does not run `cat`), so the slurp itself
# adds nothing.
hub=docs/README.md
[ -f "$hub" ] || { echo "check-docs-link-graph: missing $hub" >&2; exit 1; }
hub_text="$(< "$hub")"
orphans=""
docs_pages=0
while IFS= read -r f; do
  rel="${f#docs/}"
  [ "$rel" = "README.md" ] && continue
  docs_pages=$((docs_pages + 1))
  [[ $hub_text == *"$rel"* ]] || orphans="$orphans $rel"
done < <(git ls-files -- 'docs/**/*.md' 'docs/*.md')
# Subject-set floor (2026-07-29): all three checks in this file enumerate through a git pathspec, and
# "no orphans found" and "no pages enumerated" produce the identical green. Plain counters, not
# ${#arr[@]} — under `set -u` an empty array's expansion reports "unbound variable" instead of the
# reason, and a guard that fails with the wrong message costs the next reader the whole diagnosis.
if [ "$docs_pages" -eq 0 ]; then
  echo "check-docs-link-graph: FAILED -- enumerated ZERO docs pages besides $hub. The docs/ pathspec" >&2
  echo "  matched nothing, so check 1 proved nothing about hub reachability. An empty subject set is a" >&2
  echo "  broken guard, never a clean hub." >&2
  exit 1
fi
if [ -n "$orphans" ]; then
  echo "check-docs-link-graph: docs pages not referenced from docs/README.md (orphaned from the hub):" >&2
  printf '    %s\n' $orphans >&2
  fail=1
fi

# --- Check 2: every entry directly under an examples hub is referenced from that hub's README ---
# Two hubs, not one, since examples/ split into adapters/ + cases/ (2026-07-26): checking only the
# top level would ask nothing but "are `adapters` and `cases` mentioned", and the nine adapter
# entries this check exists for — the 2026-07-16 drift was examples/README.md listing 6 of 9 —
# would silently stop being guarded. cases/ is deliberately NOT a hub: its entries are fixture data
# (trees/, EXPECTED.jsonc, config), not pages a reader navigates to.
for exhub in examples/README.md examples/adapters/README.md; do
  exdir="${exhub%/README.md}"
  [ -f "$exhub" ] || { echo "check-docs-link-graph: missing $exhub" >&2; exit 1; }
  exhub_text="$(< "$exhub")"
  orphans=""
  entries=0
  # The `sed | sort -u` that used to sit after `git ls-files` is done in bash instead: strip the
  # directory prefix, cut at the first "/" to get the top-level entry name, and de-duplicate through
  # an associative array. Identical result, and it is three fewer processes per hub on top of the
  # per-entry grep this loop no longer spawns (see check 1's note). Iteration order is git's own path
  # order rather than `sort -u`'s, which only reorders the orphan list a failure prints.
  declare -A seen_entry=()
  while IFS= read -r path; do
    entry="${path#"$exdir"/}"
    entry="${entry%%/*}"
    [ -n "$entry" ] || continue
    [ -n "${seen_entry[$entry]:-}" ] && continue
    seen_entry["$entry"]=1
    [ "$entry" = "README.md" ] && continue
    entries=$((entries + 1))
    [[ $exhub_text == *"$entry"* ]] || orphans="$orphans $entry"
  done < <(git ls-files -- "$exdir/*" "$exdir/**")
  unset seen_entry
  if [ "$entries" -eq 0 ]; then
    echo "check-docs-link-graph: FAILED -- enumerated ZERO entries under $exdir besides its README." >&2
    echo "  The hub's own pathspec matched nothing, so check 2 proved nothing for this hub. The 2026-07-16" >&2
    echo "  drift this check exists for (6 of 9 adapters listed) is invisible to a scan of zero entries." >&2
    exit 1
  fi
  if [ -n "$orphans" ]; then
    echo "check-docs-link-graph: $exdir entries not referenced from $exhub:" >&2
    printf '    %s\n' $orphans >&2
    fail=1
  fi
done

# --- Check 3: every local Markdown link resolves — target file exists, #fragment names a heading ---
# One awk pass over every tracked *.md (a per-file loop costs minutes under Windows msys process
# spawning). External schemes and non-.md targets are out of scope: this guard owns the prose graph,
# not the network and not the schema fixtures docs link to.
#
# The zero-file case is the sharpest of the three here, and it was measured 2026-07-29: with the `*.md`
# pathspec redirected in a scratch copy, `awk` was invoked with NO FILE ARGUMENTS AT ALL, so it read
# STDIN, saw EOF, and the guard printed "OK (... 0 local md links resolve, 0 of them anchored against 0
# headings)" and exited 0. A green that says "0 links resolve" is not a clean tree, it is a scan that
# never happened — and awk's file-or-stdin default is what turns the broken pathspec into silence.
# Building the list with a counted loop rather than `mapfile` also keeps `"${md_files[@]}"` provably
# non-empty at the awk call, so `set -u` never has an empty array to complain about.
md_files=()
md_count=0
while IFS= read -r f; do
  [ -n "$f" ] || continue
  md_files+=("$f")
  md_count=$((md_count + 1))
done < <(git ls-files -- '*.md')
if [ "$md_count" -eq 0 ]; then
  echo "check-docs-link-graph: FAILED -- enumerated ZERO tracked *.md files. Check 3 would hand awk no" >&2
  echo "  file arguments, awk would read stdin, and this guard would report '0 local md links resolve'" >&2
  echo "  as a pass. An empty subject set is a broken guard, never a clean link graph." >&2
  exit 1
fi
link_report="$(awk '
  function slugify(s,   t) {
    sub(/^#+[ \t]*/, "", s)
    sub(/[ \t]+#+[ \t]*$/, "", s)
    t = tolower(s)
    gsub(/[^a-z0-9 _-]/, "", t)
    gsub(/ /, "-", t)
    return t
  }
  # Fold "a/b/../c" and "./" away so a target resolves to the same string git ls-files printed.
  function normpath(p,   n, i, parts, out, k, r) {
    n = split(p, parts, "/"); k = 0
    for (i = 1; i <= n; i++) {
      if (parts[i] == "" || parts[i] == ".") continue
      if (parts[i] == "..") { if (k > 0) k--; continue }
      out[++k] = parts[i]
    }
    r = ""
    for (i = 1; i <= k; i++) r = (i == 1 ? out[i] : r "/" out[i])
    return r
  }
  FNR == 1 { infence = 0; known[FILENAME] = 1 }
  /^```/   { infence = !infence; next }
  infence  { next }
  /^#{1,6}[ \t]/ {
    base = slugify($0); s = base; n = seen[FILENAME "|" base]++
    if (n > 0) s = base "-" n
    slug[FILENAME "|" s] = 1; nslug++
    next
  }
  {
    line = $0
    while (match(line, /\]\([^) \t]+\)/)) {
      target = substr(line, RSTART + 2, RLENGTH - 3)
      line = substr(line, RSTART + RLENGTH)
      if (target ~ /^(https?:|mailto:|ftp:)/) continue
      h = index(target, "#")
      if (h > 0) { path = substr(target, 1, h - 1); anch = substr(target, h + 1) }
      else       { path = target; anch = "" }
      if (path == "") tgt = FILENAME
      else if (path !~ /\.md$/) continue
      else {
        dir = FILENAME
        if (!sub(/\/[^\/]*$/, "", dir)) dir = ""
        tgt = normpath(dir == "" ? path : dir "/" path)
      }
      nlink++
      if (anch != "") nfrag++
      pending[++np] = FILENAME "\t" target "\t" tgt "\t" anch
    }
  }
  END {
    for (i = 1; i <= np; i++) {
      split(pending[i], a, "\t")
      if (!(a[3] in known))            { print "  dead file link:   " a[1] " -> " a[2]; bad++; continue }
      if (a[4] == "")                  continue
      if (!((a[3] "|" a[4]) in slug))  { print "  dead anchor:      " a[1] " -> " a[2]; bad++ }
    }
    printf "STATS %d %d %d %d\n", nlink, nfrag, nslug, bad + 0
  }
' "${md_files[@]}")"

link_stats="$(grep "^STATS " <<< "$link_report")"
link_bad="$(grep -v "^STATS " <<< "$link_report" || true)"
if [ -n "$link_bad" ]; then
  echo "check-docs-link-graph: Markdown links that do not resolve:" >&2
  printf '%s\n' "$link_bad" >&2
  echo "  A link target is a claim about the repo; fix the path, or the heading the #fragment names." >&2
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "check-docs-link-graph: FAILED — add the missing hub reference (or remove the orphan)." >&2
  exit 1
fi
# shellcheck disable=SC2086
set -- $link_stats
echo "check-docs-link-graph: OK ($docs_pages docs pages + 2 examples hubs reference every entry; $md_count md files scanned, $2 local md links resolve, $3 of them anchored against $4 headings)"
