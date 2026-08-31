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

**`zzop coverage`'s recognizer channel vocabulary gained a fourth spelling, and three rows changed
which one they carry.** `io.provides:db-table` used to name the db kind on BOTH sides of the join;
it now names table/model **declarations** only, and `io.consumes:db-table` names queries against a
table. Consequences on the wire, in `trees[].ioChannels.zeroExtraction[].channel` and in
`frameworkRecognizers[].emits`: `prisma client`, TypeScript's `raw sql` and Rust's `raw sql` now emit
`io.consumes:db-table` where they emitted `io.provides:db-table`; `typeorm`, `django`, `sqlalchemy`
and `gorm` gained a second row for the consume side they always filled. A consumer that groups by the
`io.provides:db-table` string will see three recognizers leave that bucket. Only three channels are
measured as `zeroExtraction` rows (`io.provides`, `io.consumes`, `io.provides:db-table`) — that set
is unchanged, and no row count moved on the 9-tree corpus. What moved is which recognizers a row
NAMES: measured on immich, the `(io.provides:db-table, ts)` row went from `["prisma client", "raw
sql", "typeorm"]` to `["typeorm"]`. The old list was false in a direction that inverts the row's
meaning — that tree holds 77 `CREATE TABLE` statements across 24 `.ts` files and extracted 0, so
"this build has a `.ts` raw-SQL table recognizer and it found nothing" reads as "the tree declares no
tables in TypeScript" when the truth is that this build does not read `CREATE TABLE` inside `.ts` at
all. `findings.*` and `--fail-on` exit codes are untouched.

**`react/setstate-after-async-unguarded` moved from `warning` to `info`.** If you gate on
`--fail-on warning`, findings from this rule no longer contribute to exit `3`; they still appear in the
reply and still count in `findings.total`. The rule's own message had, for several releases, carried a
sentence disqualifying its own dominant finding shape — "a plain event handler is mounted by
construction whenever it fires, so a `setX(...)` inside one is an accepted false positive here" — which
is a gate described rather than implemented. Measured over a 9-tree corpus, 34 of its 49 findings sat in
an event handler rather than in a `useEffect` callback. The gate itself is NOT in this release: the
matcher cannot ask whether a line sits inside a hook callback, because projected function bodies are
anonymous line spans with no record of which call receives a function as its argument. The severity is
therefore the disclosure made machine-readable, and it moves back to `warning` if and when that gate is
built. The rule's message also stopped naming the "state update on an unmounted component" console
warning as its headline symptom without qualification: React removed that warning in 18.0.0.

**`zzop analyze --fail-on <severity>` is new, and it introduces exit code `3`.** Given the flag, a run
whose `findings.bySeverity` holds anything at or above the threshold exits `3` after printing the
whole reply on stdout, with one counts-and-threshold line on stderr. Without the flag nothing
changes: the exit code still answers only "did zzop run", so a tree full of criticals exits `0`. `3`
rather than `1` on purpose — `1` already means zzop could not answer, and a CI log has to tell a
broken config apart from a real finding. `cross --fail-on` is REFUSED (exit 2) rather than accepted
and never fired: that reply carries no per-tree severity census, so the gate would silently cover the
cross-layer half alone. Verify against your own build rather than this sentence:
`zzop analyze <tree> --fail-on critical; echo $?`.

**`zzop analyze --rule <id>` can now exit 2 where it used to exit 0.** A filter this run can PROVE
could not match is refused on stderr instead of being accepted silently. Three lanes: a bare id
naming neither a bundled DSL rule nor a native analysis (refused at argv time); a `<pack>/<rule>` id
whose pack is absent from the reply's own `packsLoaded`; and — new, and the one a pipeline is most
likely to be carrying today — a `<pack>/<rule>` id whose pack IS in `packsLoaded` and which that
pack's own `ruleIds` does not contain, i.e. a typo inside a pack that loaded. Both post-run lanes are
decided from the reply ahead of the `--fail-on` gate, so 2 outranks 3. Nothing that passes a real id
changes — and a bare DSL id, which used to filter NOTHING because every DSL finding's `ruleId` is
`<pack>/<rule>`, now resolves to its full form and works. If a pipeline passes a rule id that never
matched, it turns red; that is the point, since the old answer was byte-indistinguishable from
"nothing to report". Check yours the same way: `zzop analyze <tree> --rule <the id you pass>; echo $?`.

Both entries above are recorded even though *The compatibility surface* does not name EXIT CODES
among its four surfaces — it neither covers them nor lists them under *Explicitly NOT part of the
compatibility surface*. That gap is real and unsettled; until it is settled, an exit-code change that
can turn a green pipeline red is written down rather than left for you to discover, which is what
this file is for.

**A repeated finding message is now stored once per reply, and two reply keys are new.** In a reply
where two or more findings in `shown` carry the exact same `message` text, that text moves to a
`ruleMessages` object sitting beside the `shown` list it came from, keyed by the rule id (a rule with
more than one repeated text gets `<ruleId>#2`, `#3`, ... for the later ones), and each of those
findings gains a `messageRef` holding its key. A `ruleMessagesMeaning` sentence rides once alongside.
`message` itself is still a non-empty string on every finding — never empty, never a bare id — but on
a folded finding it is a short pointer rather than the prescription, so **a consumer that reads
`message` and ignores `messageRef` will print the pointer where the prescription belongs.** The rule
is: read `ruleMessages[finding.messageRef]` when `messageRef` is present, `finding.message` directly
when it is absent. The object is a SIBLING of the `shown` list rather than a fixed path, because one
shaper feeds both the `findings` and the `crossLayerFindings` blocks.

Folding is conditional, not universal: it is applied only where storing the text once removes more
bytes than the pointer, the `messageRef` field and the table itself cost to add. A repeated message
left inline is a size decision and never a sign that anything was left out — `truncated` remains the
only key that ever means that. Measured on cal.com, the `analyze` reply goes 3,186,439 -> 1,697,723
bytes; on a small or rule-filtered reply the fold usually does not fire at all.

All three keys are additive and absent rather than empty when they do not apply, so a consumer that
never learns them keeps working on every reply where nothing folded — and, on replies where
something did, keeps working only if it already tolerated `message` wording changing between
versions, which VERSIONING.md places outside the compatibility surface.
**`analyze` and `coverage` gained reply fields.** `analyze` carries `coverageGaps` (always present —
`{basis, extensions, meaning}`: the principal filetypes whose code reached no resolved import edge,
with each row carrying `{ext, files, kind, structural}`, plus the `basis` sentence that makes an
empty list readable as measured rather than unasked and a `meaning` sentence that says what a zero
can and cannot mean). `analyze`'s `architecture` object gained `topRecommendationMeaning` beside the
`painMeaning`/`criticalTopMeaning` it already shipped — the sentence saying that
`topRecommendation.severity` is a PRIORITY BAND computed from structure alone, so a `critical` band
next to a `bySeverity` holding no critical is the normal case rather than a contradiction. Every
`packsLoaded` entry gained `ruleIds`, unconditionally present: the LIST behind the `rules` COUNT, so
a consumer validating a rule filter can answer "is there such a rule here" instead of guessing from
the pack prefix (this is what the third `--rule` lane above is decided from). `coverage` carries
`ioChannels` (one row per io kind this build's rules read, PRESENT EVEN AT ZERO, plus a
`zeroExtraction` cross of extractor capability against the tree's own filetype mix) and
`unreadExtensions`, whose rows are `{ext, kind, sharePct}`. `blindSpotBasis` keeps its name and its
type; its sentence now states what the cross EXCLUDED, not only what it included. Read the shapes off
your own build rather than off this paragraph:
```sh
zzop analyze <tree> | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{
  const r=JSON.parse(s), k=o=>Object.keys(o||{}).join(" ");
  console.log("packsLoaded[]            ", k(r.packsLoaded[0]));
  console.log("coverageGaps             ", k(r.coverageGaps));
  console.log("coverageGaps.extensions[]", k((r.coverageGaps||{}).extensions?.[0]));
  console.log("architecture             ", k(r.architecture)); })'
```

**Not everything in that paragraph is additive, and the non-additive half changes numbers you may
already have captured.** Two value changes, both in the direction of reporting MORE:

- **`coverageGaps` and `unreadExtensions` list extensions they used to omit.** Eligibility moved off
  the "would anyone write a parser adapter for this filetype?" list and onto a separate question,
  "was anything LOST by having no structural projection here?" — so structured data and configuration
  filetypes (`.xml`, `.json`, `.yaml`, lockfiles, …) now qualify and arrive labelled `kind:
  "data-config"`, while prose/image/binary filetypes still do not. This is why the rows carry `kind`
  at all: a `data-config` zero and a source-language zero take different remedies. A consumer that
  treated a non-empty list as "a parser is missing" must now read `kind` first. Recount with the
  `node` line above; the two answers differ on any tree carrying a data/config filetype above the
  principal floor.
- **`coverage`'s `extensions[]` rows flip `structural`/`lexicalOnly` for io-only parsers.** A file
  whose parser projects io facts and nothing else — `.sql` and `.prisma` — was counted `lexicalOnly`,
  whose own legend promises no parser projected any symbol, import or io fact from it. Those rows now
  read `structural`: measured on a three-`.ts`-plus-one-`.sql` tree, that `.sql` row is
  `{"structural":1,"lexicalOnly":0}`. The old answer for the same shape is quoted, with the field
  measurement that forced the change, in the regression test that now pins it
  (`a_file_whose_parser_projects_io_only_is_structural_not_lexical`, `crates/facade/src/query_coverage/tests.rs`).
  Re-read your own trees with `zzop coverage <tree>` rather than trusting a captured copy.

Additive fields alone are a MINOR change under [VERSIONING.md](VERSIONING.md)'s *The compatibility
surface*, so no promise moves for those — recorded anyway for the one consumer shape they break: a
schema validator pinned with `additionalProperties: false` against a captured reply will reject these
runs. The two value changes above move no field name or type either, but they DO move the contents of
a reply, so a golden-file test over `coverageGaps`, `unreadExtensions` or `coverage.extensions` needs
re-baselining. Nothing else in this window has touched a covered surface, and nothing in one was
removed or repurposed.

**`security/open-redirect` no longer fires when a validating helper produces the redirect's WHOLE
target, so a run that used to fail `--fail-on warning` can now pass it.** Same direction,
and written down for the same reason, as the two `duplicate-route` entries below: this turns a RED
pipeline green. Unlike those, the finding is not demoted — it is GONE, so a consumer counting rows
sees the drop directly.

The rule prescribed "validate the target against an allow-list" and then could not read one. cal.com
routes its redirect targets through an origin allowlist (`packages/lib/getSafeRedirectUrl.ts` — a
non-listed origin is discarded and replaced with `WEBAPP_URL`) at 37 of its redirect sites, and every
one of them still reported. A prescription that cannot turn its own finding green is what this change
removes. A call whose name carries a safety word (`safe`/`sanitize`/`allowlist`) AND a target word
(`Url`/`Uri`/`Redirect`/`Link`) now clears the finding — but only where it decides the whole target:
it must open the redirect's argument AND nothing that could move the origin may follow it. The value
may continue into `??`/`||` fallbacks that are themselves calls or quoted literals, and the helper
may be interpolated at the START of a template literal whose remaining text begins with `?` or `#`,
since a query string and a fragment both terminate a URL's authority. It is read inside the redirect
call's OWN parentheses — the anchor line plus the continuation lines those parens hold open, capped
at 8 — because 36 of those 37 have the helper on a CONTINUATION line, where a formatter put it. A
veto that read the anchor line alone would have cleared 1 of the rule's 34 corpus findings.

The fallback arm admits a fallback by SHAPE, not by safety, and the rule message now says so: a
`??`/`||` fallback is trusted because it is a call, so `getSafeRedirectUrl(base) ?? String(req.query.next)`
is cleared and is an open redirect. 36 of the 37 mitigated sites use that arm and none of them puts a
request read in a fallback, so the arm stays and the cost is disclosed rather than erased.

Measured on calcom/cal.com: 28 -> 15. The controls did not move — expressjs/express 4, nocodb 2, and
the sibling `browser/location-assign-dynamic` unchanged tree-wide. Of the 28, 16 anchors go quiet, 3 findings RE-ANCHOR onto a
second, unmitigated `redirect(` in the same function, and 12 are untouched. The 15 that survive are
the point of the number: `packages/app-store/stripepayment/api/paymentCallback.ts:90` still fires, and it is
the genuine one — its `callbackUrl` reaches `res.redirect` through a zod `.parse()` whose
`.transform()` only prefixes a base URL onto RELATIVE targets and passes an ABSOLUTE `http(s)://`
one through unchanged. Schema parses are deliberately absent from the vocabulary for exactly that
reason, and a regression test pins it
(`a_zod_parsed_query_url_that_passes_absolute_urls_through_is_still_flagged`,
`rules/dsl/security/open_redirect_veto.rs`).

**What it does NOT claim.** A helper standing beside a raw request value rather than replacing it
still fires, in both directions and in both spellings — `getSafeRedirectUrl(base) + req.query.q` and
`` `${getSafeRedirectUrl(base)}${req.query.next}` `` are open redirects (with `q = "@evil.com"` the
browser reads the allowlisted prefix as userinfo and the authority becomes `evil.com`), and the veto
is written so that it does not reach them. Out of reach, and so still firing, by construction: a
validator applied on a PRECEDING statement or decided by a wrapper, one written below a comment
inside the argument list, one whose value does not close within 8 lines, and a line carrying two
`redirect(` calls — that last declines the veto outright rather than guessing which call it belongs
to. The rule's message names all of them, so a surviving finding says why it survived. One FALSE
VETO is stated rather than fixed: `sanitizeUrl` (`@braintree/sanitize-url`) carries both halves of
the vocabulary while doing an XSS job — it strips `javascript:`/`data:` protocols and passes an
absolute `https://evil.com` through — so it clears a redirect it should not, and the rule's message
says so. The vocabulary is a built-in default, not a config key: a DSL matcher's patterns compile
from the pack JSON before any config resolves, so `vocabulary.*` is unreadable from here, and the
debt is recorded on the `convention` axis of `scripts/dsl-inline-census.txt`.

Recount for your own tree: `zzop analyze <tree> --rule security/open-redirect --limit 50`.

`method-scan` gains a `trigger_call_exclude_pattern` field on the rule-pack contract
([docs/contracts/rule-pack.schema.json](docs/contracts/rule-pack.schema.json),
[docs/rules/dsl-reference.md](docs/rules/dsl-reference.md)) — additive, so no promise moves, and an
existing third-party pack is unaffected.

**`duplicate-route` now reports a version-split pair at `info` instead of `warning`, so a run that
used to fail `--fail-on warning` can now pass it.** Same direction, and written down for the same
reason, as the deployment-unit entry below it: this turns a RED pipeline green. Nothing is dropped —
the finding is still raised, still names both sites, and now names the two version scopes it
straddles in a new `data.routeVersions`.

The pair it covers is one the rule could not see at all. A framework may version an API by HEADER
rather than by URL — NestJS's `VersioningType.CUSTOM` is the measured case — and then two controllers
that answer at different versions share one `METHOD /path` key, which the rule read as a collision.
Its prescription made that worse than a false positive: it led with "merge the handlers or remove the
duplicate", and on a header-versioned API deleting the older controller breaks every client pinned to
that version while merging is not expressible, because the two handlers take different request bodies
by design. Where both sites declare a version scope and the two DIFFER, the finding is now demoted and
the merge imperative is gone from its lead.

Measured on calcom/cal.com: all 15 findings move `warning` -> `info`, and a tree whose only warnings
were version-split pairs flips exit 3 -> 0. The controls did not move — nocodb 9, eShop 13, immich 11,
mall 7, expressjs/express 23, koel 0 — and nocodb's one genuine shadow
(`packages/nocodb/src/controllers/extensions.controller.ts:52`, two `@Get` paths that normalize to the
same key inside one unversioned controller) stays at `warning`.

**What it does NOT claim.** zzop carries the version expression as written, not as resolved — the
values are identifiers behind two hops of `as unknown as` casts across workspace packages, and
resolving them is a guess this rule declines to make. So two different version TEXTS are not proof
that the two scopes are disjoint, and the message says so rather than telling you the split is safe.
A method-level `@Version()` override is still unread; the class scope is what gets stamped.

Recount for your own tree: `zzop analyze <tree> --rule duplicate-route --limit 50` and group the
`shown[].severity` field; the demoted rows carry `data.routeVersions`.

`IoProvide` gains an optional `routeVersion` on the envelope input contract
([docs/NORMALIZED_AST.md](docs/NORMALIZED_AST.md)) — additive, so no promise moves, and recorded here
for the same consumer shape as the additive fields above: a schema validator pinned with
`additionalProperties: false` against a captured reply will reject these runs.

**`duplicate-route` now reports a cross-deployment-unit pair at `info` instead of `warning`, so a
run that used to fail `--fail-on warning` can now pass it.** This is the mirror of the entry above and
is written down for the same reason — except that this direction turns a RED pipeline green, which is
the failure this project cares about more. Nothing is dropped: the finding is still raised, still
names both sites, and now names the two deployment units it straddles.

The pair it covers is one the rule states it cannot check. Its own message has always said the
shadowing it warns about only happens "IF both registrations end up on the same router in the same
running process", and a directory is not a deployment unit. Where two sites resolve to different
deployment manifests (`pom.xml`, `*.csproj`, `composer.json`, `package.json`, `go.mod`, …), that
condition is measured to be false rather than merely unproven, and a gate wants evidence. Measured on
macrozheng/mall — four `@SpringBootApplication` classes on four ports with no pom dependency between
them — all 7 findings move to `info` and the tree's `warning` census drops 26 -> 19; on dotnet/eShop
one of 13 moves. A tree whose only warnings were cross-unit pairs flips exit 3 -> 0.

Recount for your own tree: `zzop analyze <tree> --rule duplicate-route --limit 50` and group the
`shown[].severity` field; the straddling rows carry `data.manifestBoundaries`.

**`dead-candidates` no longer reports files a build config names as an entry point**, and `.md` pages
now contribute dependency-graph in-edges. Both narrow an existing finding set rather than changing a
field: entry paths written as quoted literals inside a config this run already recognized as a config
(`vite.config.ts`, `vite.config.sw.js`, `.vitepress/config.mts` and the like) are read instead of
discarded, and an `import` inside a `<script>` block in a Markdown page — VitePress compiles each page
to a Vue SFC — counts as the edge it is. Measured on koel `3f5213d4`: 7 findings down to the 1 true
positive on a fresh clone (the audit run reported 2, the extra being `public/sw.js`, a build artifact a
clone does not carry — the reacquisition recipe now names this tree and that difference). Exact finding
sets are not a compatibility surface (see [VERSIONING.md](VERSIONING.md)), so this is recorded as a
heads-up for pinned baselines, not as a break.

Two limits on the Markdown half are worth knowing before you read a diff of your own docs tree. A
fenced code block is NOT an edge — a ```` ```vue ```` example is stripped before the scan, because the
same value seeds the `unreachable` entry set and an entry seeds a forward closure, so one documentation
sample would otherwise silence every file it transitively reaches. And a page whose prose mentions
`` `<script>` `` above a real block loses that block's imports rather than gaining wrong ones: the
extract is lexical and pairs the first opening tag with the first closing one. Neither can invent an
edge; both can withhold one.

**Seven false-positive classes measured on three outside repositories are gone, and one Prisma
projection defect behind them is fixed.** Three engineers who had never seen this tool set up their
own environments on `meilisearch/meilisearch`, `calcom/cal.com` and `apache/superset`, read the
findings against the real code, and said which ones they would actually act on. Everything below
narrows a finding set; none of it changes a field or a rule id.

| tree | findings | what stopped firing |
|---|---|---|
| meilisearch `577f7af` | 22 -> 19 | `security/hardcoded-secret` 3 -> 0 |
| calcom/cal.com `176037d` | 2683 -> 2613 | `schema/unreferenced-field-name` 65 -> 28, `schema/unreferenced-model-name` 44 -> 15, `schema/missing-timestamps` 56 -> 53 |
| apache/superset `e7dccd4` (frontend) | 536 -> 528 | `sql/truncate-in-app-code` 4 -> 0 (all four were `critical`), `browser/location-assign-dynamic` 8 -> 6, `dead-candidates` 19 -> 17 |
| apache/superset (backend) | 24 -> 23 | `sql/destructive-migration` 3 -> 2 |

The control populations do not move: the 17-tree corpus join is byte-identical, and
`corpus/frameworks/django` — which carries real `'TRUNCATE TABLE "BACKENDS_PERSON";'` and
`"DELETE FROM \`backends_person\`;"` literals — is unchanged at 305 findings with an identical
per-rule census.

The individual changes, and what each was measured against:

- **A quoted two-word English phrase is no longer a SQL statement.** `sql/truncate-in-app-code` and
  `sql/delete-no-where` accepted a case-insensitive keyword, so `t('Truncate Metric')` and
  `t('Delete from list')` matched — the first at `critical`, which means `--fail-on critical` broke a
  build on a checkbox label. The keyword must now be spelled in UNIFORM case (`TRUNCATE`/`truncate`).
  Across the corpus plus superset, 117 lines matched the old pattern: the 12 with a mixed-case keyword
  were all prose, the 105 with a uniform-case one were all SQL. A real statement written
  `"Truncate table users"` is now missed; there were zero such lines in the 117.
- **An arrow parameter named `location` is no longer a navigation sink.** `browser/location-assign-dynamic`
  excluded `=` after the assignment but not `>`, so `location => {` read as `location = >`.
- **`$VAR` and `${VAR}` count as placeholders.** `security/hardcoded-secret` excluded a bare
  `"LLM_API_KEY"` but reported `"$LLM_API_KEY"`, which is the spelling an OpenAPI example uses.
- **A Prisma model reached through its client delegate is referenced.** Prisma lowercases the first
  letter to build its accessor, so `model UserPassword` is used as `prisma.userPassword` and the
  declared name appears nowhere in correct code. `schema/unreferenced-model-name` now accepts the
  derived camelCase spelling.
- **A Prisma relation navigator is never an unreferenced field.** It is the required opposite side of a
  `@relation`, so removing it — which the finding advised — makes `prisma validate` fail outright.
  `schema/unreferenced-field-name` skips a field whose declared type names another model in the schema;
  a genuinely dead scalar column in the same model still reports.
- **`@default(now())` finally counts as a creation timestamp.** It always should have — the catalog said
  so and the rule's code tried to honour it — but the Prisma attribute projection stopped at the first
  `)`, so `@default(now())` reached the rule as the argument `now(`. Attribute arguments now survive one
  level of nesting, which also repairs `@default(uuid())`, `@default(cuid())` and
  `@default(autoincrement())` for any future consumer.
- **A file directly inside a dot-directory is tool-owned whatever its stem.** `.storybook/main.mjs` was
  reported as a dead file because the exemption required the stem `config`; Storybook writes `main`,
  `preview` and `manager`. Measured before widening over 13,509 js/ts-family files: 10 files newly
  exempt, every one inside a `.storybook/`. The depth-1 boundary is what carries the claim, so a nested
  `docs/.vitepress/theme/index.ts` is still ordinary source.

Everything else in the window: `git log <the top row's tag>..main --oneline`.

## Released

| Version | Date | Commit | What the release said it was |
|---|---|---|---|
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
