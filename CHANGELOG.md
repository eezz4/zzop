# Changelog

zzop is pre-1.0 (`0.x`), so this is not yet a Semantic Versioning changelog — the version number
alone still does not tell you whether an upgrade breaks you, and
[VERSIONING.md](VERSIONING.md) is where that policy lives. What this file is: **the record that
VERSIONING.md's *The compatibility surface* promises.** A break to one of the surfaces that section
names is written down here, old and new spelling both, so that finding out is not your job.

Two rules keep this file checkable rather than believable:

- **Nothing here is reconstructed prose.** Every released row carries its tag, the release commit,
  its date, and that commit's own subject line — with only the leading release stamp dropped
  (`v0.30.0 — `, `release: v0.3.0 — `), because the Version column already carries it. No row is
  reworded. The releases are the record; this page indexes them. Where a row's headline is not
  enough, the per-tag notes are on the
  [GitHub releases page](https://github.com/eezz4/zzop/releases).
- **A row appears when a tag is cut**, not when work lands. Until then a change sits under
  [Unreleased](#unreleased), and rolling that section into a row is part of cutting the release.

Tags are cut by CI on the remote, so a clone that has never fetched them is stale — fetch before
regenerating the table, for the reason VERSIONING.md gives under *Breaking in the current `0.x`*:

```sh
git fetch --tags origin
for t in $(git for-each-ref --format='%(refname:short)' refs/tags); do
  printf '%s\t%s\n' "$t" "$(git log -1 --format='%h %cs %s' "$t^{commit}")"
done
```

## Unreleased

Work on `main` past the top row below, so an id or a file named here may not be one an installed
`zzop` knows yet.

**A release does not write its own row — the step that cuts the next one folds it.** So a version can
be installable while the table below still ends at its predecessor, and an installed `zzop version`
reading higher than the top row is the documented state rather than a gap in this file.

**A DSL finding's `message` no longer repeats prose that its rule already owns.** A finding whose
text is exactly what its rule alone would have produced — that rule's declared `message` plus the two
sentences the engine appends — now carries a short pointer and a new `messageBy` field instead, and the
full text is reached by the finding's own `ruleId`: `zzop explain <ruleId>` on the CLI, or the
`zzop://rule/{id}` resource template on MCP. **Detect the lane by the `messageBy` field, never by the
sentence** — the field names the resolver and is the contract; the sentence is wording, which
[VERSIONING.md](VERSIONING.md) keeps outside the compatibility surface. Both serve the same bytes from the same function, and
that function now also prints the disable knob, which used to ride only on the finding. Measured through the CLI on
`cases/trees/api-be` at the default window (`zzop analyze cases/trees/api-be | wc -c`):
**127,937 → 62,110 bytes, -51.5%**. 46 of the 50 shown findings carry the field; the 4 that keep their
prose are native. The MCP lane measures differently because its envelope carries JSON-RPC framing —
take the number from whichever surface you actually read.

**This is a break if you were reading the prose out of `message`.** The field is still a string and
still present, so nothing that checks its type or its presence moves — but a consumer that grepped a
sentence out of it, or rendered it as the explanation, now gets the pointer. Read the full text by id
instead. *Four classes are unaffected and keep every byte inline*: native analyses (they have no
declared message to restore from); findings whose text was rewritten to name a suppression comment
the rule does not honour (that token comes from your source and is in no other field); **any rule
this binary does not carry** — a pack in `zzop/rules/` or `packs.extraDirs` keeps its prose, because
the two resolvers above answer only for compiled-in rules and pointing at them would name a door
that replies "unknown rule id"; and any rule whose whole message is cheaper than the pointer. **No finding is removed and no count moves** —
`total`, `bySeverity` and `byRule` never read `message` — and `truncated` is still the only key that
means rows were left out. The reply discloses this itself in `findings.messageByIdMeaning`, which
rides only when some finding carries the pointer. `zzop_engine::analyze_tree`, the Rust library entry
point, is unchanged and still returns the full prose.

**Six rules moved `warning` → `info`, and one of them lost a second band.** `circular`,
`schema/god-model`, `schema/nullable-fk`, `schema/model-churn`,
`cross-layer/db-table-name-in-multiple-sources` and `cross-layer/external-host-in-multiple-sources`
report a SHAPE rather than claim a defect, and each already said so in its own shipped text: `circular`
ends by naming a legitimate way to keep the cycle ("if this cycle is an intentional, reviewed
pattern"), `god-model` calls its 15-field line "a convention, not a measurement", `nullable-fk` opens
by calling the optional side a choice the schema made, and the two join rules concede the coincidence
they report may be expected in the reader's stack. Measured over three fixed corpus trees, this class
held **12 of the 150 first-screen rows — and all 12 were `circular`**; on the cross-repo reply
`db-table-name-in-multiple-sources` held **6 of 50**. After the move those first screens carry **zero**
rows from the class, and each is byte-identical to the same run with the rules demoted by config.
**No finding is removed and no count moves**: `findings.total` and `findings.byRule` are unchanged on
all four measurements. What changes is `bySeverity` (on one tree `warning` 418 → 306) and therefore
what `--fail-on warning` breaks on — gate on `--fail-on info`, or re-raise the ones you trust with
`severityOverrides`.

`schema/model-churn` additionally **lost its `critical` tier**, which escalated past an injected churn
count of 10. The rule's own message calls that line "a round number with no measurement behind
it" and tells the reader to judge the raw `data.count` instead; a line the code itself calls
unmeasurable must not hand out the loudest band. The count and the reporting line (5) are untouched,
and the rule remains silent by construction in every native run — nothing injects that attribute.


**`--rule` now refuses the seven ids that gate a pass instead of reporting one.** `schema-structural`,
`schema-usage`, `seams`, `scores`, `health`, `recommendations` and `criticality` are real ids — config
`rules.disabled` speaks them, and disabling one switches its whole pass off — but no finding is ever
keyed by them, so a view filtered to one came back `shown: 0` with no warning beside a full census.
Measured on `cases/trees/api-be`: all seven returned an empty list over 92 findings, exit 0, empty
stderr. That is the same "a filter reading as a clean result" the unknown-`--rule` warning already
existed to end; it was checking whether the id is REGISTERED, which these are.

The reply now carries a warning naming the id, saying the empty list is the filter, and pointing at the
id space that does carry findings — `schema/<label>` for the two family gates, the score surfaces for
the other five. **No exit code changes and no finding moves**; `--fail-on` is untouched, since it reads
`findings.bySeverity` and never the filtered list. A filter naming a rule that simply found nothing —
`--rule schema/god-model` on a clean tree — stays silent, which is the case this must not break.


**Three analyses now ship OFF, and turning one on takes no new vocabulary.** `unimported-export`,
`dead-candidates` and `unreachable` are unused-code hygiene rather than defect claims; measured across
the dogfood corpus those three alone were **61.7% of every finding a run reported**, and the knip tool
produces the same list for a JS/TS project as its whole job. A first run whose top half is hygiene
buries the findings that claim a defect, and every evaluator who read one of those runs ended up
writing an allowlist by hand. **To turn one on, name it in `rules` with a severity** —
`"dead-candidates": "info"` — which is the same gesture that changes any other rule's band.

Nothing is removed from the build, and the reply says which analyses were not evaluated: a new
`nativeAnalyses.shippedOff` list, beside the `disabled` one it is deliberately NOT folded into. The
two mean the same non-evaluation under different authorship, and the split is the point — `disabled`
is what your config chose.

**This replaces the starter-configuration version of the same change, earlier in this same unreleased
window, and the reason is measured.** A starter config is written by `zzop init` into a NEW tree, so
its reach into a tree that already has a `zzop.config.jsonc` is zero: the six dogfood trees kept
reporting all 2,570 of those findings after it landed. Writing them as `"off"` also attributed the
project's opinion to the reader, who would find three ids under `ruleOverridesApplied.disabled` and
`nativeAnalyses.disabled` that they never wrote. The starter config's `rules` block is now empty and
its comment explains the default instead of restating it.

**Breaking for a config that inherited the default.** A run that wants any of the three must now name
it; a run that already disabled them by id is unchanged, and so is a library caller building an
`EngineConfig` directly, which still gets every registered analysis. This repo's own detection corpus
is one of the callers that had to opt in (`cases/zzop.config.jsonc`).

**A guard holds the six documents that enumerate the set.** Which analyses do not run is invisible in a
reply's `findings` — a shipped-off analysis produces no key, exactly like one that ran clean — so prose
is the only channel that says so, and a stale copy does not read as stale: it reads as *"this id runs by
default"* and sends a reader to debug a rule that never ran. `scripts/check-shipped-off-sync.sh` reads
the engine's own set and requires each document's claiming paragraph to name exactly it, in both
directions, so a REMOVAL from the set fails too.

The starter document also moved out of a Rust string literal into
`crates/config/src/config-template.jsonc`, embedded with `include_str!`.

**`truncated.severitiesNotShown` now says how many RULES a silenced band hid, not only how many rows.**
When the cap removes a severity outright, `counts` gave the row count and three sample rows. That reads
as a handful of noisy rules and is often wrong: on this repo's own `cases/trees/api-be` fixture the cut
silences 30 `info` rows drawn from **19 distinct rules**, and the three samples name three of them. The
new `ruleCounts` is exact, is filled from the same walk as `counts` and `firstOmitted` so the three can
never describe different sets, and matters because the rule — not the row — is what you act on:
`--rule`, a `rules` entry and a `zzop-<id>-ok` marker are all keyed by it. Additive; nothing else in the
reply moved, and the cut itself is unchanged.

**27 rules moved from `warning` to `info`, and `--fail-on warning` will go quieter because of it.**
The band is supposed to say how strong a rule's own evidence is. These 27 already said, in their own
shipped messages, that their evidence is two independently matched halves with nothing linking them —
*"the two halves are matched independently within one function body and nothing establishes that they
belong to the SAME call"*. A rule that spells out a disqualifying condition it cannot detect is
describing a gate it did not build, and the place for that fact is the severity, not the prose. Each
of the 27 now says so in its message and names the specific link it cannot prove.

**12 of the 27 are `security/` rules** — `taint-flow`, `path-traversal` and `java-path-traversal`,
`ssrf-user-url`, `open-redirect`, `mass-assignment`, `html-response-from-request`,
`stacktrace-to-response`, `unsafe-deserialization`, and the three command-injection spellings
(`cmd-injection`, `command-and-interpolation`, `command-interpolated-string`). The other fifteen, named too, because a count is not a migration note: `db/external-call-and-tx`,
`db/find-then-create-no-unique`, `db/empty-catch-and-write`, `db/multi-write-no-tx`,
`db/non-atomic-counter-update`, `db/check-then-act-in-loop`, `db/manual-tx-no-rollback`,
`db/tx-and-empty-catch`, `db/tx-and-db-call-in-loop`, `redis/lock-get-then-set`,
`redis/counter-get-set`, `sql/race-condition-toctou`, `sql/raw-sql-check-then-write`,
`egress/get-and-body`, and `reliability/promise-all-and-writes`. **Nothing was removed and no finding count
changed**: the same findings report, under a band that now describes their evidence instead of their
topic. If a pipeline gated on these, gate on `--fail-on info`, or raise back the ones you trust with
`severityOverrides`. A rule returns to `warning` when the link it names actually gets built.

**Rules that were NOT moved, so the change is not "co-occurrence goes quiet".** Seven rules disclose a
limit and keep `warning`, because their finding is ONE structurally matched shape and the disclaimer is
about taint or about the scope of a veto, not about a missing link: `security/sql-format-interpolation`
and `security/sql-interpolated-statement` (the literal IS the statement), `security/weak-password-hash`
(the digest is parser-witnessed), `security/bcrypt-cost-too-low`, `security/cors-reflected-origin-credentials`,
`db/pagination-no-orderby` and `reliability/reqwest-no-timeout`. Ten more that say *"not merely
co-occurring — verified against the parser's projected loop spans"* were never candidates: that
sentence says the gate EXISTS.

**A guard now ties the published severity to the shipped one.** `docs/rules/catalog.md` kept saying
`warning` for all 27 with every guard and the whole test suite green — the catalog was compared to the
site and the site to the catalog, and nothing compared either to the packs. `scripts/check-catalog-severity-sync.sh`
closes that.

**Three packs that shipped one rule each were merged into `reliability`, so three rule ids changed**
(`go/goroutine-in-loop`, `perf/api-in-loop`, `react/setstate-after-async-unguarded` are now
`reliability/goroutine-in-loop`, `reliability/api-in-loop`,
`reliability/setstate-after-async-unguarded`). Nothing was removed and no rule changed what it
detects: the three JSON objects moved file, and the bundle went from 11 packs to 8 while the rule
count stayed at 118. A pack whose whole content is one rule tells a reader nothing the rule id did
not already say, and each charged a `packs.only`/`packs.disabled` name, a catalog section and a site
section for that. `reliability` was already the pack for work started and not finished
(`fs-in-loop-serial`, `stream-open-no-close-in-loop`, `interval-no-clear`) and was already
multi-language (`reqwest-no-timeout` is Rust), so the axis it merges onto is CONCERN, not language.
**If you carry one of the old ids** in `rules`, `suppressions`, `--fail-on`, or a `zzop-<id>-ok`
marker, re-spell it: the rename rows are in [VERSIONING.md](VERSIONING.md). `packs.only: ["go"]`,
`["perf"]` or `["react"]` now names no loaded pack and switches off every DSL rule — the run warns,
but the warning is the only thing telling you, so it is worth grepping your config for those three
words. Suppression markers are derived from the rule id and are therefore affected too.

**`security/shell-exec-interpolation` reports `warning` rather than `critical`.** Its two
cross-language siblings, Java `security/cmd-injection` and Rust `security/command-and-interpolation`,
already did. The gap rested on `exec`/`execSync` handing their whole command string to a shell, which
is true and is why the argv rewrite is the fix — but it is a property of the API rather than evidence
about the call, and this family's band is decided by reachability, which this rule states in its own
message that it does not test. Measured across a 58-tree census, its fourteen `critical` findings
included zero whose interpolated segment was request-derived. The finding, its text and its matcher
are otherwise unchanged, and the message's closing paragraph now argues for the band it actually has.
A `--fail-on critical` gate that this rule alone was tripping will stop tripping; `--fail-on warning`
is unaffected.
**The `N file(s) with extension .<ext> have no native parser` warnings are now ordered by unread file count, largest first.** They came out in extension-name order, which ranked this run's blind spots by how they are spelled: measured on the dogfood corpus, nocodb's `.vue` line (962 unread files) was the 42nd of 49 `warnings` entries, below eight extensions of 1-10 files each; koel's `.php` (1412) sat 10th, one line under a single `.psd`; immich's `.svelte` (415) sat 25th of 38. Ties break on the extension name, so the order is total and two runs over one tree stay byte-identical. The closing `No native parser exists for N extension(s) in this tree (…)` summary samples the same order, so the five extensions it names are now the five largest rather than the five alphabetically first — that sentence is the one line whose text changes. Nothing is added, dropped, shortened or re-worded otherwise: the set of warnings and every other line's bytes are unchanged, as are `total`, `bySeverity`, `byRule` and every finding. A consumer that reads these entries by index rather than by content will see different strings at those indices. The sibling `coverageGaps.extensions` table keeps its extension-ascending order.

**A `--fail-on` build that breaks now says WHERE.** Additive field: `findings.truncated.severitiesNotShown.firstOmitted`, an object keyed by the severities the cap removed from `shown` outright, each holding that band's first cut rows as `ruleId`/`file`/`line` in the same order `shown` uses. It is a bounded sample (at most 3 a severity; the sibling `counts` stays exact) and carries no `message`. The gap it closes was measurable at the exit code: on a large tree `zzop analyze --limit 1000 --fail-on critical` exited 3 naming `6 critical` while the 1000-row reply it had just printed carried none of them, because ordering puts test and build surface behind shipped code and the cap took the whole band. `zzop analyze --fail-on` now prints those sites on stderr whenever the reply's own list holds none of the rows it gated on, and names the caller's own `--rule`/`--severity` filter when that is what kept them out. Nothing else moved: ordering, the cap, `total`, `bySeverity` and `byRule` are unchanged, and the gate still decides on `bySeverity` alone.

## Released

| Version | Date | Commit | What the release said it was |
|---|---|---|---|
| `v0.34.0` | 2026-08-31 | `4c504978` | the reply stops repeating itself, and the rules start saying what your fix costs |
| `v0.33.0` | 2026-08-15 | `73951a2` | fix(site): x-showcase row filter so site-render-check passes |
| `v0.32.0` | 2026-08-15 | `dd52eae` | the co-change picture stops dropping edges silently, and the site now shows zzop run over everything X open-sourced |
| `v0.31.0` | 2026-08-14 | `10adb51` | subtree git history, wildcard routes, and accessor/overload spans |
| `v0.30.0` | 2026-08-10 | `e7b20da` | zzop was built TS-only; this release is what did not survive the expansion |
| `v0.29.1` | 2026-08-05 | `890e355` | the release lane can be re-run, which one of its two publish jobs could not |
| `v0.29.0` | 2026-08-05 | `b9dfa3f` | rules read call structure instead of guessing from text, and the graph answers what sits on top of what |
| `v0.28.0` | 2026-08-01 | `668841d` | the analyzer says which of your stack it recognizes, and admits when a key is wrong rather than merely missing |
| `v0.27.0` | 2026-07-31 | `908f107` | an adapter can correct the parser, and `exclude` finally moves the number |
| `v0.26.1` | 2026-07-29 | `2c77c9c` | the version that actually ships 0.26.0 |
| `v0.26.0` | 2026-07-29 | `3a129fa` | exclude means "do not name this path", the calling side can declare its base, and no fingerprint is bumped by hand any more |
| `v0.25.0` | 2026-07-28 | `2a15d35` | config is mandatory, undeclared vocabulary makes no judgment, and the lane that ships releases stopped bypassing every gate |
| `v0.24.0` | 2026-07-26 | `acb9890` | rules read declarations instead of guessing, caching is on by default, and three surfaces stopped lying about themselves |
| `v0.23.0` | 2026-07-25 | `72890e0` | rule naming/taxonomy BREAKING, plugin installs itself, and four silent wrongs fixed |
| `v0.22.0` | 2026-07-24 | `a637bd9` | packaging/docs cleanup — product-level asset naming, plugin mcpServers, discoverable privacy, npm badge |
| `v0.21.1` | 2026-07-23 | `c6f8e10` | npm CLI revived as zero-logic native-binary packaging, product-layer restructure, new perf/concurrency rules, parser-rule reachability contract |
| `v0.21.0` | 2026-07-23 | `4af12ac` | two binaries + version-SSOT + package-version cache stamps; CRITICAL retrying-write cross-layer rule; framework-neutral http security rules (whole-tree IoScan); pages-api/Python precision fixes |
| `v0.20.0` | 2026-07-21 | `79eb059` | "everything is injection" routing doctrine, native C#, ORM db-table complete, npm removed |
| `v0.19.0` | 2026-07-18 | `69d5777` | per-app fetch-egress census, intra-file wrapper joins, parser-sql version parity, clippy-1.97 fix, napi naming accuracy, CI 17->3 jobs |
| `v0.18.0` | 2026-07-18 | `88b9b9a` | db-table join channel (SQL + Prisma), Java auth routes, Mode A on zzop-mcp, pure-facade hosts |
| `v0.17.0` | 2026-07-17 | `dd1aec0` | Rust & Go native parsers, Java lexical -> full CST, Python full-AST tier; MCP host grows to 5 tools / 9 contract resources |
| `v0.16.0` | 2026-07-16 | `7de1a97` | Node-free MCP host, overlay self-disclosure, per-extension "bring an adapter" diagnostics |
| `v0.15.0` | 2026-07-15 | `0f2f079` | generic entity-attribute injection channel + concern-first rule packs (breaking) |
| `v0.14.0` | 2026-07-14 | `1d23bc7` | deployment topology + field-driven precision, deterministic-gate positioning |
| `v0.13.0` | 2026-07-13 | `abdc146` | contract-honesty release — body-field-drift, axios baseURL keying, silent no-op sweep, docs audit |
| `v0.12.0` | 2026-07-13 | `8cbb0b5` | cross-layer reach — manual-dispatch provides + base-carrier consume keying, driven by liberation field review |
| `v0.11.0` | 2026-07-12 | `1392961` | structural loop-containment matcher — precision release driven by mono-hub field review |
| `v0.10.0` | 2026-07-12 | `a6a3d78` | cross-repo resolution reach, prefix-drift, adapter kit, drift guards |
| `v0.9.0` | 2026-07-11 | `6bcad90` | rule expansion wave (37 new rules), cross-layer generalization, failOn integration, disclosure tripwires |
| `v0.8.0` | 2026-07-10 | `5b3bb37` | cross-layer keying completions, full message audit, redis pack, message-contract machinery |
| `v0.7.0` | 2026-07-10 | `05ceb10` | ci(prebuild): pin publish npm to the 11.x line — npm@12.0.0 breaks --provenance |
| `v0.6.0` | 2026-07-09 | `734ec40` | coverage census + blindness disclosure, JSX-in-.js parsing, wrapper-adapter |
| `v0.5.0` | 2026-07-08 | `2fddf1c` | feat(cross-layer): route-near-miss — actionable "did you mean" for drifted consumes |
| `v0.3.0` | 2026-07-07 | `8955edb` | CLI report output, SDK docs, cross-layer facts in parser IR, dep-graph edge accuracy |
| `v0.2.0` | 2026-07-06 | `40c488f` | fix(engine,cli): sharpen as-cast/dead-candidates precision, fold info output, add glob excludes |
| `v0.1.0` | 2026-07-06 | `9e123c5` | fix(napi): add repository field to platform packages for provenance |

There is no `v0.4.0`: the tag list skips it, and no release ever carried that number.

Four rows (`v0.1.0`, `v0.2.0`, `v0.5.0`, `v0.7.0`) read as a Conventional-Commit subject rather than
a release headline. Those releases were cut from an ordinary commit, before the
one-commit-per-release convention settled, and the subject is reproduced as it stands rather than
rewritten into something the commit did not say.
