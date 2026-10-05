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

Nothing on the compatibility surface broke since `v0.37.0`. That was **measured rather than
assumed**, and each surface below carries what it was measured with — so a reader who doubts the
sentence can re-run the check rather than take it.

- **CLI JSON output.** `node scripts/measure/wire-key-census.mjs` was run against a `v0.37.0`
  binary (built from `git archive v0.37.0`) and against this tree **with the same script**, so a
  change in the instrument could not read as a change in the surface: **504 key paths then, 525 now —
  21 added, 0 removed.** One qualification, because "the same script" is the whole weight of that
  sentence: the baseline run used a copy with the two new `LANE_ASYMMETRY` waivers deleted. The
  census refuses a waiver that waives nothing, and against a binary predating the key it waives
  nothing by construction — so without the deletion the baseline cannot be taken at all. That list
  is read only by the sibling-lane parity check and never by the path walk, so the counted surface
  is identical either way; the path lists were also diffed outright rather than only their totals.

  🔴 **And that baseline disagrees with the one this file published last time.** The `v0.37.0` row's own
  `## Unreleased` text said **508** for the tree that became `v0.37.0`, and the archive build measures
  **504**. The discrepancy could not be reproduced away: nothing between the commit that wrote "508" and
  the tag touches the wire (`git log bd910c6..v0.37.0` is two commits — `.gitignore`, `ci.yml` and one
  guard), the census script is byte-identical across the range, and `cases/` is unchanged. So one of the
  two numbers was wrong when written, and on the evidence it is the older one, which was asserted
  rather than re-derived from an archive build. What the compatibility claim rests on is not either
  total but the **path-list diff**, which is direct: no path present at `v0.37.0` is missing now.

  Two of the added are SCHEMA: `findings.ruleCaveats` and its legend
  `findings.ruleCaveatsMeaning`. The rest are `findings.ruleCaveats.<ruleId>` entries, which this census
  walks the same way it walks `ruleMessages.<ruleId>` — they move with the corpus a fixture happens to
  hold, not with the surface. Both new keys are additive-only (absent, never empty) and shaped exactly
  like `ruleMessages`/`ruleMessagesMeaning`: an object keyed by rule id beside a string legend.

  What they carry is new, though, and it is worth a sentence because it is the first thing this
  release gives a reader back rather than takes away. `v0.34.0` folded rule prose out of the reply, and
  a rule's text is half *what the defect is* and half *when that reading is wrong* — the fold removed
  both, so the claim reached the first screen and the refutation did not. `ruleCaveats` puts the second
  half back, once per DISTINCT rule shown rather than once per finding, which is why it fits inline
  when the full messages did not. 📏 `node scripts/measure/caveat-cost.mjs corpus/audit/{immich,nocodb,cal.com}/zzop.config.jsonc`
  on the three audit trees at the default window: the full messages would have cost **52-80%** of
  the reply, these clauses cost **8-11%**, and they cover **62-72%** of the findings on the screen.
  Bounds rounded outward — the script prints them to a tenth, and a tenth moves when a legend is
  reworded, so the prose keeps the figure a reader can rely on and the command keeps the exact one. That script fixes the metric as well as deriving it — the two cost figures first
  published for this feature were taken through different windows and compared to each other. A rule's ABSENCE from the map is never a claim that the finding is
  right: it means no caveat was written down, OR the rule is a native analysis with no field to
  declare one, OR it comes from a pack this binary does not carry. In the last two cases the
  finding keeps its `message` inline, so the clause is already in the reader's hands. The reply's own
  legend states all three.

- **Rule ids.** `bash scripts/check-rule-id-renames-recorded.sh`, which diffs the catalog against the
  newest tag read off the REMOTE: **178 ids, none retired since `v0.37.0`.**
- **Rule pack schema.** `docs/contracts/rule-pack.schema.json` gained one OPTIONAL property, `caveat`
  (`["string", "null"]`). A pack that omits it is unchanged and still validates. Where a rule declares
  it, the value must be a VERBATIM SUBSTRING of that rule's own `message` — enforced byte for byte by
  `scripts/check-rule-caveat-substring.sh`, so the field can never state something the message does not.
- **Findings cache (not a compatibility surface, but a cost this release charges).** Upgrading
  invalidates every findings-cache entry, once. The ruleset fingerprint hashes the whole
  `RulePackDef` debug form, so a new `RuleDef` field moves it even for packs that declare none of
  it — measured on a 98-file tree: `hitFiles` 98/98 before, **0/98** on the first run after a
  caveat-only edit, 98/98 again on the next. IR cache is unaffected (the ruleset is not in its key),
  so this is one cold rule-evaluation pass, not a cold parse. Over-invalidation is the safe
  direction and narrowing the fingerprint is a correctness change, so it is not being done inside a
  release audit; the measurement and the precedent for narrowing are recorded in the backlog.
- **CLI flags & config keys.** Nothing added, removed or repurposed
  (`git diff v0.37.0..HEAD -- crates/config/config-surface.json` is empty).
- **CLI exit codes `0`/`1`/`2`.** Unchanged in meaning and in what earns each one.
  ⚠ Three codes, not four: `--fail-on`'s `3` is deliberately OUTSIDE the compatibility surface and
  `VERSIONING.md` says why — it moves when a rule's severity band moves, so one verdict covering all
  four would have to be wrong about one half. This bullet said `0`/`1`/`2`/`3` in draft, which
  silently promised the thing that document exists to refuse.
- **Normalized AST envelope.** The shape and its `version` are untouched. `docs/NORMALIZED_AST.md` changed
  in this window, but the edit REMOVED a claim rather than adding one: the sentence listing five `client`
  tags the native TS parser emits was a partial copy that drifted the moment a sixth landed, and it now
  points at the `frameworkRecognizers` cell of `zzop coverage` — the authoritative set — instead of
  restating part of it.

The window's own work was one product repair and the corrections around it: the `explain` lane had kept a
CLI-only parity exemption after gaining an MCP wire in September, two published disclosure numbers were
wrong where the mechanism that would have caught them already existed, and the site's graph page claimed
nothing wires its counts to CI when CI has wired them since July.

## Released

| Version | Date | Commit | What the release said it was |
|---|---|---|---|
| `v0.37.0` | 2026-10-03 | `606bbc9` | fix(ci): both v0.37.0 CI failures were green locally for different reasons |
| `v0.36.0` | 2026-09-23 | `6649e8a` | the MCP registry gets the release it refused, and the limit it refused on is now read locally |
| `v0.35.0` | 2026-09-23 | `a1ac023` | fix(guards): the rule-id rename guard reads its baseline, and reads the whole id |
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
