#!/usr/bin/env bash
# Mechanical census of every policy-shaped constant under the crates that hold rule/extraction logic
# (crates/engine/src, crates/core/src, parser/*/src, rules/native/*/src). This is the "continuous
# drift review" mechanism (A5): the census tracks EXISTENCE (path:NAME), never values — values change
# legitimately and are not what this guard is for. Its job is to force a triage moment — assigning
# one axis from the vocabulary below ($axes: fact/convention/cap/internal/test) — every time a *new*
# policy-shaped name is introduced, by failing CI until the committed snapshot
# (scripts/policy-census.txt) is regenerated to include it. (The T1/T2/T3 TIERS are the policy-value
# inventory's own axis, documented where a constant is declared — this census does not record them.)
#
# ## Sibling axis: DSL pack `${NAME}` fragments — censused in Rust, not here
# A Rust-const-only census has a structural evasion hole, not just a gap: policy vocabulary MOVED out
# of a Rust `const` and into a pack's `fragments` map would bypass the triage moment at zero cost, and
# an escape hatch that cheap always gets used eventually. So `${NAME}` fragment NAMES are censused
# too — but by `zzop_core::dsl::tests_fragments::name_census`, which reads them through `serde_json`,
# NOT by this script.
#
# That closure was NAME-DEEP ONLY, and this block claimed more than it held until 2026-08-02: a policy
# value written INLINE in a matcher field has no name, so it was invisible to both censuses and moving
# vocabulary into one was still a free bypass. Measured: `sql/nplus1`'s root anchor
# (`^(?:domains/[^/]+/routes/.+|api/.+)`) shipped with no triage record anywhere and made a flagship rule
# structurally silent under `src/api/`. A THIRD census now owns that axis —
# `zzop_core::dsl::inline_census_tests` -> `scripts/dsl-inline-census.txt`, one row per
# `(pack/rule, field, value)` triple, walking whatever `RulePackDef::expand_fragments` walks so a new
# matcher field cannot enter the packs outside it. Also Rust, also for the JSON reason above.
#
# That axis lived here for one day (2026-07-25) as a line-oriented `awk` extractor over
# `rules/dsl/**/*.json`, and this header asserted the line orientation "fails LOUD, never silent".
# Measured false, on inputs that are all VALID JSON, with no JSON formatter guard anywhere in this
# repo to prevent them: a second key appended to the line after `"fragments": {` was dropped SILENTLY
# (census output unchanged -> guard green -> triage moment bypassed, i.e. the evasion route reopened),
# a key sharing the opening-brace line was dropped, and a minified map yielded a PHANTOM name from the
# surrounding object rather than the promised `removed:`. Fixing the `awk` would mean a hand-rolled,
# string-aware, brace-depth-tracking JSON tokenizer in `awk` — fragment values are regexes, and
# shipped ones already contain a `"` (that module owns the count) — one crate away from a real parser this repo already runs
# over those exact files. The subtraction was to delete the second parser, not improve it. See that
# module's header for the full table and the rejected alternatives.
#
# What stays here is the Rust half, and its line orientation is ENFORCED rather than assumed:
# `cargo fmt --all --check` is a CI gate, so a `const` declaration cannot be reflowed onto a shared
# line the way a hand-edited JSON key can.
#
# ## Scan roots are DERIVED, not listed (2026-07-29) — and it took three misses to get here
# The roots used to be a hand list: `crates/{engine,core,config,summary}/src` plus two globs. The
# entries below record the same accident happening over and over, each time fixed by APPENDING one
# more crate to that list:
#
#   crates/metrics/src is DELIBERATELY out of scope for THRESHOLDS (2026-07-16): SEAMS_MIN_FILES and
#   friends are metric eligibility/presentation floors, not rule/extraction policy vocab. Its NAME
#   vocabulary is scanned — see the name-only block further down for how that exclusion had to be
#   narrowed to the axis it actually argued.
#
#   crates/facade/src stays out of scope (2026-07-16): its QUERY_*_LIMIT consts are result-truncation
#   caps on already-computed output. crates/summary/src JOINED the scan (2026-07-17, facade-thinning
#   batch): the shared summary layer's DEFAULT_*_LIMIT / MAX_LIMIT caps moved there from the deleted
#   host crate (previously excluded as per-host presentation) — now that every host shares them, a
#   silent cap change alters what EVERY agent-facing surface shows.
#
#   crates/config/src JOINED the scan (2026-07-27, crates/host teardown batch). It was the one
#   workspace crate in NEITHER list — not scanned, and not recorded as a reasoned exclusion the way
#   crates/metrics and crates/facade are above — "so its absence was an oversight rather than a
#   decision", as this header put it at the time.
#
# And then it happened AGAIN, to the two crates that sentence did not think to check. Measured
# 2026-07-29: crates/cache/src and crates/git/src were in neither the scan nor the exclusions, and
# nine census-shaped constants lived there unseen —
#     crates/cache/src/lib.rs:TOOL_DIR (".zzop"), DEFAULT_CACHE_DIR (".zzop/cache")
#     crates/cache/src/store.rs:SCHEMA_VERSION_FILE, IR_DIR, FINDINGS_DIR, FORMAT_VERSION
#     crates/git/src/process.rs:COMMIT_MARKER, crates/git/src/parse.rs:MS_PER_DAY, ...
# A guard that had already WRITTEN DOWN this exact mistake committed it again, because the fix it
# applied was to the list rather than to the listing. So the list is gone: the scan roots are derived
# from Cargo.toml's `[workspace] members` (see the derivation below for why that rather than a
# directory glob), and the two buckets there are the only way out. A new crate is therefore born IN
# the census — its constants arrive as `added:` drift with axis `?`, which fails the check until
# someone triages them. That is the triage moment this whole script exists to force, and it now fires
# for a crate nobody remembered to think about.
#
# The declared exclusions are asserted to EXIST on disk. A bucket entry pointing at a renamed crate
# would otherwise be inert: `excluded` silently stops excluding (harmless — the crate joins the scan
# loudly) but `name_only` silently stops scanning metrics ENTIRELY, and a shrink that the drift
# comparison reports as `removed:` lines is a diagnosis nobody would connect to a typo in a bucket.
#
# packages/cli-bin/src and packages/mcp/src are still NOT scanned, but that is now a WRITTEN exclusion
# instead of a silence — which is the whole difference this entry is about. packages/mcp/src/server.rs's
# SUPPORTED_PROTOCOL_VERSIONS is census-shaped, and the reason it stays out is unchanged: those two
# crates are product/distribution shells, admitting them is a scope question whose answer governs every
# future const under them, and it deserves its own decision rather than riding along with a derivation
# fix. What changed is that the derivation now REACHES them, so declining them had to be said out loud
# in $excluded_dirs rather than achieved by not thinking about them.
#
# Regex covers `^\s*(pub[(vis)] )?const NAME: (&str|&[&str]|&[(&str,&str)]|[&str; N]|usize|u32|i32|f64) = ...`.
# This is tighter than "every const" on purpose — it's scoped to the shapes that actually carry policy
# (string vocabularies and small numeric thresholds). Two closed blind spots (both 2026-07-13, v0.12.0
# release audit): `[&str; N]` fixed arrays (HTTP_VERB_EXPORTS / PAGES_API_FALLBACK_VERBS / compose VERBS
# carried verb policy invisibly in that form) and scoped visibility (`pub(crate)`/`pub(super)`/
# `pub(in ...)` const — the audit's own WRITE_HTTP_METHODS unification escaped a `(pub )?`-only pattern
# on the visibility axis). If a future sweep needs to widen the type list further, re-run --update and
# re-check the line count; if it balloons, narrow back down and note that here.
#
# ## Bare `&str` joined 2026-07-27 (148 -> the committed snapshot's entries)
# A single `const NAME: &str = "literal"` sat outside the alternation for two releases. It was measured
# in the v0.13.0 audit (~32-35 new entries, mostly `PARSER_FINGERPRINT`-shaped) and DEFERRED as
# ballooning: the census was sized to stay "modest", and the sentinel-kind desync that motivated it had
# been closed a different way (`NEST_GLOBAL_PREFIX_KIND` / `CLIENT_BASE_PREFIX_KIND` became T1-shared
# single-source consts).
#
# That trade was re-judged and reversed. The gap was not "some fingerprint strings" — it was an entire
# SHAPE of vocabulary: every regex-valued name vocabulary. `DEFAULT_AUTH_GUARD_PATTERN` (which decides
# whether a mutating route counts as guarded, i.e. the largest single lever over this repo's findings)
# plus the three auth-acquisition path patterns beside it were `0` of the 148 tracked entries. A census
# whose stated job is "force a triage moment for every new policy-shaped name" cannot omit the shape its
# highest-consequence names are written in — the fact that a census EXISTED made it read as complete.
# The real count came in at +79, above the earlier estimate, because the estimate predated the config/
# and summary/ scan-dir additions and counted only module-level consts.
#
# ## The `axis` column (2026-07-27)
# Each snapshot line is `path:CONST <axis>` with an optional ` # rationale` tail:
#
#     rules/native/rules-http/src/mutating_route_no_auth.rs:DEFAULT_AUTH_GUARD_PATTERN convention # ...
#
# Existence alone was not enough. Before this column, a constant PASSED the guard as soon as it appeared
# in the snapshot, so the triage verdict the header promises ("triage it, then regenerate") was never
# recorded anywhere — the reviewer's judgment evaporated the moment `--update` ran. The column makes the
# verdict the artifact: `--update` writes `?` for any key it has not seen before, and a `?` fails the
# check. Regenerating is therefore no longer a way to make the guard green.
#
# The discriminating question for a NAME is one line: **can another project spell this differently?**
#
#   fact        A framework or language fixed the name. `@GetMapping` is Spring's, `router.post` is
#               Express's, tree-sitter node kinds are the grammar's, `.tsx` is the language's. Built in,
#               never declared: letting a user redefine these only breaks reading that framework.
#   convention  The PROJECT picks the name — guard functions, secret parameters, money fields, generated
#               -file banners, ignored directories, URL segments. These are what the engine must stop
#               guessing at; they belong in config with the built-in value shipped as a template default.
#   cap         Not a name at all — a number (`*_LIMIT`, `*_THRESHOLD`, `MIN_*`, `MAX_*`, sample sizes).
#   internal    Not a name any project chooses, because zzop itself defines it: cache/schema versions,
#               parser fingerprints, sentinel kinds, attribute keys on the producer/consumer channel,
#               rule ids, embedded blobs, warning prose. Distinct from `fact` on purpose — folding these
#               into `fact` would swell the one bucket that is supposed to mean "a framework owns this".
#   test        A fixture or expectation constant under a test module. Not shipped policy.
#
# WHEN IN DOUBT, WRITE `convention`. The two errors are not symmetric: calling a fact a convention costs
# a config key nobody sets (the built-in default keeps running, harmless), while calling a convention a
# fact leaves the engine guessing a name it had no business guessing — silent misclassification, which is
# the whole failure this column exists to surface.
#
# Rationale lives HERE, on the line, not next to the declaration. The reason is the key: `--update`
# carries the tail forward keyed by `path:CONST`, so renaming or moving a constant DROPS its axis and
# forces a fresh triage. A comment parked at the declaration would survive that rename and keep asserting
# a verdict nobody re-made. It also keeps one fact in one place — a per-declaration copy of the axis
# would be a second list to desync from this one. Only entries whose axis is not self-evident carry a
# tail; `AMBIGUOUS -` marks the ones resolved by the doubt rule above rather than by certainty.
#
# No deps beyond grep/sed/sort/comm/awk. (No JSON reader — deliberately: this repo's shells have neither
# jq nor a usable python, which is the practical half of why the fragment axis moved to Rust.)
set -euo pipefail

# Collation-pinned: the snapshot mixes lowercase paths, '/:._-' and uppercase const names — exactly
# the tokens whose sort order differs between C and UTF-8 locales. Without this, an --update run on
# one machine and a check run on another (CI) can disagree on ORDER alone and report spurious drift.
export LC_ALL=C

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

census_file="scripts/policy-census.txt"
# Tuple-shaped vocab (`&[(&str, &str)]`) joined 2026-07-17: the ORM-silence marker table matched the
# spirit of SERVER_FRAMEWORK_SPECIFIERS but its tuple element type escaped the census regex entirely.
#
# `type_alternation` is the SINGLE owner of "which const shapes this census reads". The blind-spot
# assertion at the bottom of this script re-reads it and compares against every const type actually
# present in the scanned dirs, so a shape that is neither scanned nor explicitly waived fails.
#
# `u64` and `&[u8]` joined 2026-07-29, and they are the first thing the DERIVED scan roots found. Both
# live in crates the hand-listed roots never reached, so the blind-spot assertion directly above —
# whose stated job is "the scanner must declare what it cannot see" — had never been asked about them.
# A completeness assertion is only as complete as the subject set it runs over; that is the same defect
# one level up, and it is why the roots are derived now.
#   u64    — crates/cache/src/hash.rs's FNV_OFFSET_A/FNV_PRIME_A/FNV_OFFSET_B/FNV_PRIME_B. Numeric,
#            same shape family as the usize/u32/i32/i64/f64 already read, and consequential: these
#            four values decide every cache entry's filename, so a silent edit is a repo-wide cache
#            identity change.
#   &[u8]   — a byte-string literal (`b"..."`) is a string vocabulary written in bytes. Reading it is
#            the fail-closed direction the header's doubt rule asks for; the one present instance is a
#            test fixture and censuses as `test`.
# f32 joined 2026-08-03 (A17): `HIGH_ENTROPY_SECRET_MIN_BITS` — an entropy threshold is exactly the
# "shape family as the usize/…/f64 already read, and consequential" case the note above describes.
# &[NormalizedKey] joined 2026-08-05: `NORMALIZED_VOCABULARY_KEYS`, whose left column is a list of
# `vocabulary.*` KEY SPELLINGS — a name vocabulary wearing a type alias. The alias hid `&str` from the
# shape filter, which is the same "invisible AND silent" pair the note above describes, arriving through
# a new door (a type alias rather than a new tuple shape). Read it: the key half is policy, and a key
# silently dropped from this table restores the exact silent-failure the table was built to end.
# RuleIoChannel, &[RuleIoChannel] and &[(&str, &[RuleIoChannel])] joined 2026-08-10, all three read, all
# three the `&[NormalizedKey]` case again — a name vocabulary wearing a struct type. The struct's two
# fields ARE string spellings (an `io.provides`/`io.consumes` side and an io kind), so the six named
# channel constants and the `ALL` set are string vocabulary the shape filter would otherwise not see;
# and the per-crate `NATIVE_ANALYSES` tables pair zzop's own rule IDS with them. A rule id silently
# dropped from such a table stops being registered AND stops being declared in one edit, which is the
# widest silent hole any of these shapes can open.
# &[RuleChannelDirection] joined 2026-08-13: the measured `(rule id, channel, direction)` table behind
# the empty-channel disclosure. Same rule-ID vocabulary as the `NATIVE_ANALYSES` rows above wearing a
# third struct type — and its rows are QUOTED to users by name, so a row silently dropped removes a
# rule from a published sentence about what this run could not see.
type_alternation='&str|&\[&str\]|&\[u8\]|&\[\(&str,[[:space:]]*&str\)\]|\[&str;[[:space:]]*[0-9]+\]|\[\(&str,[[:space:]]*&str\);[[:space:]]*[0-9]+\]|&\[\(&str,[[:space:]]*&\[&str\]\)\]|&\[\(&str,[[:space:]]*&\[&str\],[[:space:]]*&str\)\]|&\[\(&str,[[:space:]]*SeverityValue\)\]|&\[\(&str,[[:space:]]*&\[RuleIoChannel\]\)\]|&\[BlindnessClass\]|&\[FrameworkRecognizer\]|&\[RuleChannelDirection\]|&\[NormalizedKey\]|&\[RuleIoChannel\]|RuleIoChannel|RiskWeights|usize|u32|u64|i32|i64|f64|f32'
pattern="^[[:space:]]*(pub(\\((crate|super|in [^)]+)\\))? )?const [A-Z_][A-Z0-9_]*: ($type_alternation)"

# Const types the census DELIBERATELY does not read, one line per type with its reason. Nothing is
# excluded implicitly — an implicit exclusion is precisely the defect the blind-spot assertion exists to
# prevent (bare `&str` sat outside the alternation for two releases and nothing said so out loud).
#   char  — single-character delimiters and in-band placeholders (`\u{1f}` field separator, `\u{0}`
#           glob/param placeholder). One character cannot hold a name vocabulary, and none of these is a
#           threshold; they are parsing mechanics.
#   &'static [Language]
#         — the set of languages a config may NAME (`Language::WIRE_NAMES`). It holds no strings and no
#           numbers: the spellings themselves live in `Language::as_wire`, an exhaustive match with no
#           wildcard arm, so a new variant cannot compile without being given one. A census entry here
#           would track the enumeration rather than any policy value, and the thing worth guarding is
#           already guarded by the compiler plus a round-trip test.
ignored_types='char
&'"'"'static [Language]'

# The axis vocabulary — see the header block. `?` is deliberately absent: it is what --update writes for
# an untriaged key, and its whole job is to fail this check.
axes='fact convention cap internal test'

# ## The subject set: every Cargo workspace member's src/
# Directory globs (`crates/*/src parser/*/src rules/native/*/src`) were the first attempt at deriving
# this, and they were still a hand-shaped guess about where crates live: `rules/src` — the
# `zzop-rule-packs` member, which embeds every shipped pack — matches none of them, and would have
# been the NEXT crate to go missing exactly the way crates/cache and crates/git did. So the authority
# is `[workspace] members` in Cargo.toml, the one list that cannot go stale without Cargo itself
# failing on the next build.
#
# A new member is therefore scanned the moment it is added, without anyone remembering this file.
#
# The element pattern is ANCHORED to line start (after indentation) rather than "any quoted string on
# a line in the block", and that is not defensive style — the loose form was written first and was
# WRONG on this very manifest. Cargo.toml line 48 is a comment reading `... "rules" below is a thin
# test-only crate ...`, and the loose extractor read that prose mention as a 24th member, scanning
# rules/src twice and reporting one scan dir more than it had. Harmless only because the phantom
# happened to duplicate a real member; a comment naming a path that is NOT a member would have made
# this census scan a directory Cargo does not build. A guard whose subject set is derived from a file
# must parse that file's grammar, not grep it for shapes.
member_dirs=()
member_count=0
while IFS= read -r m; do
  [ -n "$m" ] || continue
  member_dirs+=("$m/src")
  member_count=$((member_count + 1))
done < <(awk '
  /^members[[:space:]]*=[[:space:]]*\[/ { inm = 1; next }
  inm && /^\]/ { exit }
  inm && /^[[:space:]]*"[^"]+"/ { match($0, /"[^"]+"/); print substr($0, RSTART + 1, RLENGTH - 2) }
' Cargo.toml)

if [ "$member_count" -eq 0 ]; then
  echo "check-policy-census: FAILED -- read ZERO entries out of Cargo.toml's [workspace] members list." >&2
  echo "  That list is where this census derives what to scan, so an empty read means the manifest was" >&2
  echo "  reshaped (an inline array, a different key spelling) and this guard would silently scan" >&2
  echo "  nothing. Re-point the extraction; do not hardcode the crate list back." >&2
  exit 1
fi

# The ONLY two ways a member's src/ can stay out of the full scan. Each entry is a reasoned exclusion
# argued in the header above; anything not named here is scanned, including a crate created tomorrow.
#   excluded_dirs  — read at all by neither pass.
#   name_only_dirs — string-shaped consts only; the numeric half is the declared exclusion (see the
#                    crates/metrics block further down for why that exclusion had to be split by axis).
excluded_dirs=(crates/facade/src packages/cli-bin/src packages/mcp/src)
name_only_dirs=(crates/metrics/src)

stale_bucket=""
for d in "${excluded_dirs[@]}" "${name_only_dirs[@]}"; do
  [ -d "$d" ] || stale_bucket="${stale_bucket}    ${d}"$'\n'
done
if [ -n "$stale_bucket" ]; then
  echo "check-policy-census: FAILED -- a declared scan-root exclusion points at a directory that does" >&2
  echo "  not exist:" >&2
  printf '%s' "$stale_bucket" >&2
  echo "Each entry in \$excluded_dirs / \$name_only_dirs carries a written reason in this script's" >&2
  echo "header. A stale one is inert: the name-only bucket would stop scanning that crate ALTOGETHER" >&2
  echo "and report the loss only as unexplained 'removed:' drift. Re-point it, or delete it (and its" >&2
  echo "reason) if the crate is genuinely gone." >&2
  exit 1
fi

dirs=()
dir_count=0
for d in "${member_dirs[@]}"; do
  [ -d "$d" ] || continue
  bucketed=0
  for x in "${excluded_dirs[@]}" "${name_only_dirs[@]}"; do
    [ "$d" = "$x" ] && bucketed=1
  done
  [ "$bucketed" -eq 1 ] && continue
  dirs+=("$d")
  dir_count=$((dir_count + 1))
  # BUILD SCRIPTS TOO (2026-07-29). `$member_dirs` is `<member>/src`, so a `build.rs` — which sits at the
  # crate ROOT, one level up — was outside every scan. That is not a hypothetical hole: the release audit
  # for v0.26.0 found `crates/engine/build.rs` carrying FNV basis/prime constants that ADDRESS every
  # cached entry (the same values censused as `internal` in `crates/cache/src/hash.rs`), invisible here
  # while the guard read green. A build script is exactly the kind of file this census exists for — it
  # decides values at compile time that nothing downstream can renegotiate. `find`/`grep -r` both accept
  # a file argument, so a single path needs no special casing.
  build_rs="${d%/src}/build.rs"
  [ -f "$build_rs" ] && dirs+=("$build_rs")
done

# ## Scan-root floor (2026-07-29)
# The `[ -d "$d" ]` filter above is defensive by design (the two globbed entries expand to nothing in a
# tree without parser crates), and it was reported as making a directory RENAME silently shrink the
# census. Measured 2026-07-29, and that half of the report is WRONG: a shrunken scan produces a
# `$current` that cannot equal the committed snapshot, so the drift comparison below fires
# with a `removed:` list. Pointing the whole loop at one empty directory in a scratch copy printed a
# `removed:` line for EVERY committed entry and exited 1 — loud, not silent.
#
# What is NOT covered is the total collapse, and it fails in the worst available way. With `dirs` empty,
# `grep -rE "$pattern" "${dirs[@]}"` gets no file operand and grep falls back to READING STDIN — the
# same "no arguments means read stdin" default that made check-docs-link-graph.sh certify an empty link
# graph. Measured: the guard HUNG until killed, and with stdin at EOF it walked the whole repo through
# `find` instead. A guard that hangs is a guard that gets bypassed, so the collapse is named here
# rather than left to be diagnosed from a stall. Plain counter, not `${#dirs[@]}` — see the same note
# in check-max-file-lines.sh.
if [ "$dir_count" -eq 0 ]; then
  echo "check-policy-census: FAILED -- ZERO scan directories exist. Every glob in the loop above" >&2
  echo "  (crates/*/src, parser/*/src, rules/native/*/src) matched nothing, or everything it matched" >&2
  echo "  landed in \$excluded_dirs / \$name_only_dirs, so this census would read nothing -- and a" >&2
  echo "  recursive grep with no file operand reads STDIN, which hangs rather than reporting. Fix the" >&2
  echo "  scan roots." >&2
  exit 1
fi

# ## Field-form vocabulary: a SECOND shape, scanned since 2026-07-27
# A `const` census cannot see vocabulary that lives as a STRUCT FIELD whose default is built in
# `impl Default` — `IoOptions::router_names` (`vec!["apiRoutes"]`) and `ScoresConfig::hierarchy_shared_dirs`
# (utils/types/hooks/...) are exactly that, and both were 0 of the tracked entries while sitting in
# shipped code that decides what a run extracts and how it scores layering.
#
# This is the SAME failure as the bare-`&str` gap above, one axis over: that one was a TYPE the scanner
# did not read, this one is a FORM it did not read. The blind-spot assertion below was built to prevent
# precisely this and could not, because the assertion itself only ever enumerated const TYPES — a guard
# scoped to the axis it was written on cannot see the axis it was not. Recorded rather than quietly
# fixed, because "the census exists, therefore the census is complete" is now a THREE-time failure.
#
# Extraction rule, deliberately narrow: inside `impl Default for <Type>`, a field whose default
# expression contains a STRING LITERAL directly. A field defaulting to a const (`DEFAULT_SKIP_DIRS`) or
# to another type's `::default()` is NOT emitted — the const form is already censused under its own key,
# and emitting both would double-count one vocabulary under two spellings. So this scan adds exactly the
# vocabulary that lives NOWHERE ELSE, which is the set that was invisible.
# ## crates/metrics/src: NAME shapes only (2026-07-27)
# The 2026-07-16 exclusion reads: "its THRESHOLDS (e.g. SEAMS_MIN_FILES) are metric eligibility/
# presentation floors, not rule/extraction policy vocab." That is true of thresholds and they STAY OUT.
# But the same sentence was silently load-bearing for the NAME vocabulary in the same directory —
# DEFAULT_HIERARCHY_SHARED_DIRS and the four FSD layer lists, which decide whether a project's own
# directory layout scores as a violation. Those were never "reviewed and found not to be policy"; they
# were never reviewed at all, because a rationale written about numbers governed a directory that also
# held names. A stated reason narrower than its own effect is the defect, not the exclusion.
#
# So the exclusion is narrowed to the axis it actually argued: this directory is scanned for
# STRING-shaped consts and not for numeric ones. Implemented as a type filter rather than a second
# opinion about which names matter — a hand-kept allowlist of "the metrics consts that count" would be
# the drift surface this census exists to remove.
#
# `name_only_dirs` itself is declared with the scan-root derivation near the top of this script,
# together with the on-disk assertion that keeps a rename from silently emptying it.
name_type_alternation='&str|&\[&str\]|&\[\(&str,[[:space:]]*&str\)\]|\[&str;[[:space:]]*[0-9]+\]'

# Every directory whose FIELD-form vocabulary is read: the full-scan dirs plus the name-only ones. Field
# defaults are string literals by construction (that is the extraction rule), so there is no numeric
# half to exclude here and the two lists merge without qualification.
vocab_dirs=("${dirs[@]}" "${name_only_dirs[@]}")

rust_consts() {
  grep -rnE "$pattern" "${dirs[@]}" 2>/dev/null \
    | sed -E 's/^([^:]+):[0-9]+:[[:space:]]*(pub(\((crate|super|in [^)]+)\))? )?const ([A-Z_][A-Z0-9_]*):.*/\1:\5/'
  if [ ${#name_only_dirs[@]} -gt 0 ]; then
    grep -rnE "^[[:space:]]*(pub(\\((crate|super|in [^)]+)\\))? )?const [A-Z_][A-Z0-9_]*: ($name_type_alternation)" \
      "${name_only_dirs[@]}" 2>/dev/null \
      | sed -E 's/^([^:]+):[0-9]+:[[:space:]]*(pub(\((crate|super|in [^)]+)\))? )?const ([A-Z_][A-Z0-9_]*):.*/\1:\5/'
  fi
}

# `path:Type::field` — the `::` is what tells a reader (and `grep`) a census line is field-form rather
# than a const. Multi-line aware on purpose: `hierarchy_shared_dirs`'s literals sit on the lines AFTER
# the field name, and a single-line scan silently missed it while appearing to work on `router_names`.
rust_default_field_vocab() {
  awk '
    FNR == 1 { inblock = 0; cur = ""; hit = 0 }
    /^impl Default for /{ t = $4; sub(/[ {].*/, "", t); intype = t; inblock = 1; cur = ""; hit = 0; next }
    inblock && /^}/ {
      if (cur != "" && hit) print FILENAME ":" intype "::" cur
      inblock = 0; cur = ""; hit = 0; next
    }
    inblock {
      if (match($0, /^[[:space:]]{8,}[a-z_][a-z0-9_]*:[[:space:]]*/)) {
        if (cur != "" && hit) print FILENAME ":" intype "::" cur
        f = substr($0, RSTART, RLENGTH); gsub(/[[:space:]:]/, "", f)
        cur = f; hit = 0
        if (index(substr($0, RSTART + RLENGTH), "\"")) hit = 1
        next
      }
      if (cur != "" && index($0, "\"")) hit = 1
    }
  ' $(find "${vocab_dirs[@]}" -name '*.rs' -not -name 'tests.rs' -not -path '*/tests/*' 2>/dev/null | sort)
}

current="$( { rust_consts; rust_default_field_vocab; } | sort -u)"

# ## Blind-spot assertion — the scanner must declare what it cannot see
# Every `const NAME: <TYPE>` in the scanned dirs, TYPE only. Compared against `$type_alternation` (read,
# hence censused) and `$ignored_types` (waived, with a written reason). A TYPE in neither fails. This is
# the meta-guard half: without it, a vocabulary written in an unread shape is invisible AND silent, which
# is exactly how four of this repo's highest-consequence names — `DEFAULT_AUTH_GUARD_PATTERN` and the
# three auth-path patterns beside it — sat outside a census that read as complete. Runs before --update
# too: regenerating the snapshot must not be a way to move past an unclassified shape.
all_const_types() {
  grep -rhE '^[[:space:]]*(pub(\((crate|super|in [^)]+)\))? )?const [A-Z_][A-Z0-9_]*:' "${dirs[@]}" 2>/dev/null \
    | sed -E 's/^[[:space:]]*(pub(\((crate|super|in [^)]+)\))? )?const [A-Z_][A-Z0-9_]*:[[:space:]]*//' \
    | sed -E 's/[[:space:]]*=.*$//; s/[[:space:]]*;[[:space:]]*$//' \
    | sort -u
}

# The name-only dirs get the same assertion on THEIR axis: every const type there that mentions `&str`
# is string-shaped and must therefore be inside `$name_type_alternation`. Numeric types are the declared
# exclusion for those dirs and are not asked about. Without this, widening a name vocabulary in
# crates/metrics to a shape the filter does not list (say `&[(&str, &[&str])]`) would be invisible AND
# silent — the very pair this assertion exists to break, one axis over from where it was first written.
unknown_name_types=""
if [ ${#name_only_dirs[@]} -gt 0 ]; then
  while IFS= read -r t; do
    [ -n "$t" ] || continue
    case "$t" in *'&str'*) ;; *) continue ;; esac
    grep -qE "^($name_type_alternation)\$" <<< "$t" && continue
    unknown_name_types="${unknown_name_types}    ${t}"$'\n'
  done < <(
    grep -rhE '^[[:space:]]*(pub(\((crate|super|in [^)]+)\))? )?const [A-Z_][A-Z0-9_]*:' "${name_only_dirs[@]}" 2>/dev/null \
      | sed -E 's/^[[:space:]]*(pub(\((crate|super|in [^)]+)\))? )?const [A-Z_][A-Z0-9_]*:[[:space:]]*//' \
      | sed -E 's/[[:space:]]*=.*$//; s/[[:space:]]*;[[:space:]]*$//' \
      | sort -u
  )
fi

if [ -n "$unknown_name_types" ]; then
  echo "check-policy-census: string-shaped const TYPE(s) in the name-only dirs that the filter misses:" >&2
  printf '%s' "$unknown_name_types" >&2
  echo "add them to \$name_type_alternation and run --update — a name vocabulary in a shape the filter" >&2
  echo "does not list is exactly the blind spot this assertion exists to break." >&2
  exit 1
fi

unknown_types=""
while IFS= read -r t; do
  [ -n "$t" ] || continue
  # Herestrings, not `printf | grep -q`: the SIGPIPE seal (scripts/check-shell-pipe-sigpipe.sh) forbids
  # that pipeline shape repo-wide, and a herestring has no writer process to kill.
  grep -qE "^($type_alternation)\$" <<< "$t" && continue
  grep -qxF "$t" <<< "$ignored_types" && continue
  example="$(grep -rnE "const [A-Z_][A-Z0-9_]*:[[:space:]]*$(printf '%s' "$t" | sed -E 's/[][\\.*^$(){}|+?]/\\&/g')[[:space:]]*(=|$)" "${dirs[@]}" 2>/dev/null | head -1 | cut -d: -f1,2)"
  unknown_types="${unknown_types}    ${t}    (e.g. ${example:-?})"$'\n'
done < <(all_const_types)

if [ -n "$unknown_types" ]; then
  echo "check-policy-census: const TYPE(s) the scanner neither reads nor waives:" >&2
  printf '%s' "$unknown_types" >&2
  echo >&2
  echo "Decide which it is, in THIS script, and say so in code:" >&2
  echo "  - it can carry policy vocabulary or a threshold -> add it to \$type_alternation and run --update" >&2
  echo "  - it structurally cannot          -> add it to \$ignored_types with a one-line reason" >&2
  echo "Leaving it unlisted is the third option this assertion exists to remove: a shape the census" >&2
  echo "silently does not read, in a census that reads as complete." >&2
  exit 1
fi

# Key column of the committed snapshot, in file order. The snapshot is written in `sort -u` key order,
# so this is directly comparable to `$current` — no re-sort, which would hide an ordering bug.
committed_keys() { cut -d' ' -f1 "$census_file"; }

if [ "${1:-}" = "--update" ]; then
  # The axis (and its rationale tail) is a human verdict the scanner cannot re-derive, so it is carried
  # forward keyed by `path:CONST`. A key with no prior line gets `?`, which the check below rejects —
  # that is the whole point: --update can no longer turn an untriaged constant green.
  tmp="$(mktemp)"
  printf '%s\n' "$current" | awk -v prev="$census_file" '
    BEGIN {
      while ((getline line < prev) > 0) {
        p = index(line, " ")
        if (p > 0) { tail[substr(line, 1, p - 1)] = substr(line, p + 1) }
      }
    }
    NF { print $0 " " (($0 in tail) ? tail[$0] : "?") }
  ' > "$tmp"
  mv "$tmp" "$census_file"
  count="$(committed_keys | grep -c . || true)"
  untriaged="$(awk '$2 == "?" { print $1 }' "$census_file")"
  echo "check-policy-census: snapshot regenerated ($count entries) -> $census_file"
  if [ -n "$untriaged" ]; then
    echo "  untriaged (axis '?', must be classified before the check passes):"
    printf '    %s\n' $untriaged
  fi
  exit 0
fi

if [ ! -f "$census_file" ]; then
  echo "check-policy-census: missing $census_file — run: bash scripts/check-policy-census.sh --update" >&2
  exit 1
fi

committed="$(committed_keys)"

if [ "$current" != "$committed" ]; then
  echo "check-policy-census: policy-shaped constant census has drifted from $census_file" >&2
  added="$(comm -13 <(printf '%s\n' "$committed") <(printf '%s\n' "$current") || true)"
  removed="$(comm -23 <(printf '%s\n' "$committed") <(printf '%s\n' "$current") || true)"
  if [ -n "$added" ]; then
    echo "  added:" >&2
    printf '    %s\n' $added >&2
  fi
  if [ -n "$removed" ]; then
    echo "  removed:" >&2
    printf '    %s\n' $removed >&2
  fi
  echo >&2
  echo "new policy-shaped constant — triage it by assigning one axis ($axes) on its census line and regenerate: bash scripts/check-policy-census.sh --update" >&2
  exit 1
fi

# Axis column: every line must carry exactly one axis from $axes. `?` (what --update writes for a key it
# has never seen) and a missing column both land here.
bad_axis="$(awk -v axes="$axes" '
  BEGIN { n = split(axes, a, " "); for (i = 1; i <= n; i++) ok[a[i]] = 1 }
  { if (!($2 in ok)) print "    " $1 " -> " ($2 == "" ? "(no axis)" : $2) }
' "$census_file")"

if [ -n "$bad_axis" ]; then
  echo "check-policy-census: census entries with no valid axis:" >&2
  printf '%s\n' "$bad_axis" >&2
  echo >&2
  echo "each line must read 'path:CONST <axis>' with axis one of: $axes (see this script's header for" >&2
  echo "what each means and the one question that separates them). When in doubt, write 'convention'." >&2
  exit 1
fi

count="$(printf '%s\n' "$current" | grep -c . || true)"

# Census floor (2026-07-29). Reaching here means `$current` equalled `$committed`, which is normally
# proof enough that something was read — the snapshot has the committed snapshot's entries and an empty scan cannot match
# it. The one way past that is `--update`, which rewrites the snapshot from whatever the scan found:
# run it once against a broken scan root and the snapshot becomes empty, after which every later run
# compares empty to empty and prints "OK (0 policy-shaped constants tracked; ...)". The comparison is
# self-referential, so it cannot be the floor for itself; this is.
if [ "$count" -eq 0 ]; then
  echo "check-policy-census: FAILED -- the census is EMPTY. Zero policy-shaped constants were found in" >&2
  echo "  $dir_count scan directory(ies), and $census_file agrees, so this run proved nothing. That pair" >&2
  echo "  is what a --update against a broken scan root leaves behind: a snapshot regenerated from a scan" >&2
  echo "  that read nothing. Fix the scan roots, then --update from a tree where the scan actually works." >&2
  exit 1
fi

# Fixed axis order, not awk's `for (k in array)` — that iteration order is unspecified, and a summary
# line whose field order shuffles between runs is exactly the kind of noise a guard must not emit.
by_axis="$(awk -v axes="$axes" '
  { c[$2]++ }
  END { n = split(axes, a, " "); for (i = 1; i <= n; i++) printf "%s%s=%d", (i > 1 ? " " : ""), a[i], c[a[i]] + 0 }
' "$census_file")"
echo "check-policy-census: OK ($count policy-shaped constants tracked across $dir_count scan dirs; $by_axis)"
