# examples — worked references for extending zzop

Everything here is a **runnable reference you copy from**. That is the whole rule for this directory,
and it got narrower on 2026-07-28: the labeled detection benchmark used to live under
`examples/cases/` and now sits at [`../cases/`](../cases/README.md), because it is not an example.
Nobody copies a benchmark — it is scored, in CI, by the `detection-benchmark` job. One directory
holding both "read this to learn" and "this is our ground truth" meant the second one read as
optional, and it went unscored for two years.

Third-party code we merely check out (the dogfood corpora of real OSS repositories) is somebody
else's to license and lives in the gitignored `corpus/oss/`, never here.

## [`adapters/`](adapters/README.md) — the one extension path that needs an example

Runnable references for the two authoring modes (Mode A bundle, Mode B overlay), all speaking the
Normalized-AST contract ([`docs/NORMALIZED_AST.md`](../docs/NORMALIZED_AST.md); authoring guide
[`docs/adapters/README.md`](../docs/adapters/README.md)).

**Why adapters get examples and the other extension surfaces do not.** The test is whether what zzop
ships is the same shape as what you would write:

| You want to extend | What zzop ships | Same shape? |
|---|---|---|
| A rule | `rules/dsl/http/http.json` — read by the same `parse_dsl_pack`, checked by the same `validate_rule_pack` as yours | **Yes** — the shipped packs *are* the example. Point `packs.extraDirs` at your own. |
| Vocabulary, parser routing, join topology | the `zzop.config.jsonc` that `zzop init` writes | **Yes** — the starter file is the example. |
| A parser or a middleware recognizer, natively | a Rust crate under `parser/` | No user path at all — those are ours to write. |
| **A parser or facts producer, as a user** | Rust structs and internal APIs | **No** — you write a JSON *envelope producer*, a different interface with no shipped instance. **That is why this directory exists.** |

So there is no `rule-kit` and no `parser-kit`, and adding one would mean maintaining a second copy of
something already shipped. If a surface is hard to find, the fix is a pointer, not a duplicate.
