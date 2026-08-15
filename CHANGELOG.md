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

Work on `main` past the top row below. None of it is in any released binary, so an id or a file
named here is not one an installed `zzop` knows yet.

Nothing here yet — no change in this window has touched a surface that
[VERSIONING.md](VERSIONING.md)'s *The compatibility surface* covers.

Everything else in the window: `git log <the top row's tag>..main --oneline`.

## Released

| Version | Date | Commit | What the release said it was |
|---|---|---|---|
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
