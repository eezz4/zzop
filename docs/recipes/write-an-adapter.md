# Write an adapter

**The situation.** You are an agent. zzop analyzed a tree and the answer is thin — no import edges, no
routes, a rule that says it did not run. The stack is one zzop has no native recognizer for, and it is
not going to grow one: native coverage here follows what the maintainer's own work needs (see
[ARCHITECTURE.md](../ARCHITECTURE.md)'s note on what "roadmap" means in that document). Injection is
the supported route out, and this page is how you walk it.

This is the **recipe**, not the contract. The contract — every field, every merge rule, every version
floor — is [NORMALIZED_AST.md](../NORMALIZED_AST.md) (embedded in the binary as `zzop contract
envelope-guide`) plus its [JSON Schema](../adapters/envelope.schema.json) (`zzop contract
envelope-schema`) and the key-normalization rules in [adapters/README.md](../adapters/README.md)
(`zzop contract adapter-guide`). Those stay the single source of truth for **shape**. This page adds
the four things a contract cannot give you: **which channel**, **how small**, **how to verify**, and
**when to stop**. Before you get here at all, check
[extending.md](../extending.md#7-zzop-cannot-see-your-shape-at-all) — steps 1–6 there are
declarations, and a declaration you skipped is cheaper than any program on this page.

Every number below was measured on this tree with the commands shown. Reproduce with a release build
(`cargo build --release --bin zzop`); the examples write `zzop` where a source checkout has
`target/release/zzop`.

---

## Step 1 — Which channel? Read it off the run, do not guess

Do not start from "what does my framework do". Start from what zzop already said it could not see.
`zzop coverage <tree>` is the aggregate-visibility view, and each of these signals names one channel:

| What the run says | The channel you need |
|---|---|
| `extensions[]` row with `structural` high, `declaredImports` high, **`inDepGraph` low** | dep-graph `imports` |
| `joinVisibility` with `provides: 0` / `consumesKeyed: 0` and its `meaning` naming a Mode B overlay | `io.provides` / `io.consumes` |
| warning: *"server-framework package(s) imported but only 0 http route(s) were extracted tree-wide"* | route provides — `io.provides` or `router_mount_fragments` |
| warning: *"ORM schema marker(s) detected but zero db-table io facts were extracted tree-wide"* | the db-table provide channel |
| warning: *"rule `X` did not run: it is gated on a declared `<key>` attribute"* | `attributes` |
| warning: *"N file(s) with extension `.x` have no native parser"* | whole files — every channel you care about, for those paths |

The first row is the one to internalize, because it is the quiet one. Measured on
`twitter/the-algorithm-ml` (91 `.py` files, `git clone --depth 1
https://github.com/twitter/the-algorithm-ml`), with a config declaring only that tree and
`vocabulary.skipDirs`:

```
$ zzop coverage --config ./tml.config.jsonc     # .trees[0].extensions[] row for "py"
{"declaredImports":457,"degraded":0,"ext":"py","files":91,"inDepGraph":1,"lexicalOnly":0,"structural":91}
```

91 files parsed structurally, 457 declared import specifiers, and **one** file contributing a resolved
outgoing edge. Nothing errored. That row is the whole diagnosis: the parser read the imports fine and
could not resolve them, so the channel to fill is `imports` and nothing else.

## Step 2 — The smallest envelope that helps

**Partial envelopes are legal, and "partial" goes all the way down.** A Mode B overlay may declare one
file out of ninety-one and one channel out of ten. The schema requires five top-level fields
(`format`, `version`, `parser`, `source`, `files`) and exactly two per file entry (`path`, `loc`) —
verified in [envelope.schema.json](../adapters/envelope.schema.json)'s `required` lists. Everything
else is optional and defaults to empty.

So the smallest thing that moves a number is a hand-typed JSON file with no program behind it. This
one is 18 lines, covers 1 of the 91 files above, and was written by reading three `import` lines:

```json
{
  "format": "zzop-normalized-ast",
  "version": "0.27.0",
  "parser": "hand-written/1",
  "source": "tml",
  "files": [
    {
      "path": "common/modules/embedding/embedding.py",
      "loc": 58,
      "imports": {
        "LargeEmbeddingsConfig": { "specifier": "./config", "original": "LargeEmbeddingsConfig" },
        "DataType": { "specifier": "./config", "original": "DataType" },
        "logging": { "specifier": "../../../ml_logging/torch_logging", "original": "logging" }
      },
      "overrides": { "imports": ["LargeEmbeddingsConfig", "DataType", "logging"] }
    }
  ]
}
```

Wired in, it moves `resolvedImportEdges` **3 → 5**. Write the program only once you have decided that
the hand-typed version was pointing the right way.

Two details that cost me two failed runs each, both caught by the validator rather than guessed:
`imports` values require **both** `specifier` and `original` (`original` is the exported name —
`"default"` for a default import, `"*"` for a namespace), and `loc` is required on every file entry
even though nothing cross-checks it against the file.

## Step 3 — Additive, or displacing?

An overlay **adds**: for `imports` the merge is native-first per key, so a key the native pass already
bound keeps its native binding and only keys it never bound are added. (The key is the local name the
file binds for most front ends, and something else for C# and for every import that binds no name —
`crates/core/src/ir/imports.rs` owns the per-language table, and picking the wrong key is how a
correction arrives as a sibling instead of a replacement.) That is what lets an
adapter fill exactly the gaps without owning the whole channel. It is also, on its own, not enough
whenever the native parser bound the same name to something **wrong** — which is the usual case when
resolution is what failed, because both sides are reading the same `import` statement.

Measured on the same tree, same 188 offered bindings, the only difference being the `overrides` field:

| | `resolvedImportEdges` | files in dep graph |
|---|---|---|
| native only | 3 | 1 |
| overlay, additive only (no `overrides`) | 12 | 8 |
| overlay declaring `overrides.imports` | **167** | **70** |

The additive-only run is not silently wrong — it says so:

```
adapter overlay "tml" (parser python-package-alias-adapter/1): 179 import binding(s) it offered were
DROPPED because the native parser had already bound the same local name to a different specifier and
the overlay declared no override for it: common/checkpointing/__init__.py 'Snapshot': ours
"tml.common.checkpointing.snapshot" kept, theirs "./snapshot" dropped; ...
```

179 of 188. **If you see that warning, your adapter is not additive-shaped and you need `overrides`.**
Declaring it puts a version floor of `0.27.0` on the envelope
([why floors are per-feature](../NORMALIZED_AST.md#the-overrides-version-floor)) and turns the drop
line into a displacement line naming every native fact you overrode. That line is long on purpose:
nothing verifies that the adapter is the correct side.

## Step 4 — Verify. Three failure modes, three different signals

```sh
zzop validate-envelope overlay.json     # or the validate_envelope MCP tool — same check
```

`{"valid":true,"issues":[],"hints":[]}`, exit 0. `issues` reject the envelope and decide the exit code;
`hints` are shapes that are legal but probably not what you meant, and leave a valid envelope valid.

Then wire it in and **recount**. `trees[].overlays[]` paths resolve relative to the **tree root**,
while `trees[].root` resolves relative to the **config file's own directory** — the two are not the
same base, and getting it wrong is a `configWarnings` entry, not a crash.

The three ways this goes wrong, each measured against the 18-line envelope above:

1. **Malformed** — a missing required field is a hard error before analysis starts, exit 1:
   `zzop: zzop-facade: invalid analyzeTrees() config JSON: missing field 'loc' at line 1 column 394`.
2. **Structurally invalid** — deserializes, fails validation. The overlay is skipped, the run
   completes, `resolvedImportEdges` returns to its baseline 3, and one warning says so:
   `adapter overlay 'hand-written/1' skipped: unknown format: 'not-zzop' (expected 'zzop-normalized-ast')`.
3. **Valid, applied, and pointed at nothing** — this is the one a number will not catch. Typo the path
   to `embeding.py` and `resolvedImportEdges` still reports **5**, because a declared path matching no
   file becomes a synthetic entry whose own imports resolve from its declared directory. The count
   moved; the facts landed on a file that does not exist. Only the warning distinguishes them:
   `1 of 1 declared file(s) matched no file in this tree and were added as synthetic entries`.

So the verification is two-sided: a **before/after count** and a **clean overlay warning list**. Either
alone can be green while the other is wrong.

## Step 5 — When are you done?

Stop when the signal from Step 1 has flipped, not when the adapter feels complete. Re-run the same
`zzop coverage` row and read `inDepGraph` against `declaredImports` (or `joinVisibility`, or the
framework-silence warning — whichever one sent you here). Add a second channel only when a specific
analysis you care about is still silent; the contract's own advice is the same, and each channel is a
separate budget.

---

## What it costs

Line counts of what ships in this repo (`wc -l`; the second number excludes blank and `//`-comment
lines). All of the JS ones import [`adapter-kit/`](../../examples/adapters/adapter-kit/) for the file
walk, envelope building and key normalization, so that is reuse rather than code you write:

| Adapter | Channel | Total / code |
|---|---|---|
| a hand-typed JSON file, one file entry, three bindings | `imports` | 18 / 18 |
| [`auth-overlay-adapter`](../../examples/adapters/auth-overlay-adapter/) | `attributes` | 64 / 39 |
| [`java-imports-adapter`](../../examples/adapters/java-imports-adapter/) | `imports` | 92 / 65 |
| [`python-package-alias-adapter`](../../examples/adapters/python-package-alias-adapter/) | `imports` + `overrides` | 137 / 88 |
| [`fastapi_overlay_adapter`](../../crates/engine/examples/fastapi_overlay_adapter/) (Rust) | `router_mount_fragments` | 612 / — |

Read that table as a **channel** cost, not an adapter cost. Filling `imports` or `attributes` from a
lexical scan is under 100 lines because a binding is a line of text. Filling
`router_mount_fragments` is 612 lines across three files because a router mount is a graph: prefixes
resolve across files, a non-literal prefix has to degrade honestly instead of being guessed, and the
mount target has to be resolved the way the importing language resolves it. If you are budgeting a
fragment or `io` adapter, budget the larger number.

---

## Worked example A — two minutes, no clone

Everything here is committed in this repo. It takes a tree from **0 to 2** resolved import edges.

The tree is [`test/fixture/`](../../examples/adapters/python-package-alias-adapter/test/fixture/): six
Python files where `train.py` imports the tree's own modules under the package name `mypkg`, which no
file in the tree declares.

```sh
cp -r examples/adapters/python-package-alias-adapter/test/fixture /tmp/fx
printf '{ "trees": [ { "root": ".", "sourceId": "fx" } ] }\n' > /tmp/fx/zzop.config.jsonc
zzop coverage /tmp/fx          # .trees[0].census
```

```
{"declaredImportsByExt":{"py":2},"degraded":0,"files":7,"ioConsumesKeyed":0,"ioConsumesUnresolved":0,
 "ioProvides":0,"joinContributionZero":true,"parserDispatched":6,"resolvedImportEdges":0,"symbols":3}
```

Six files parsed, two declared specifiers, zero edges — the Step 1 signal. Produce and check the
overlay:

```sh
node examples/adapters/python-package-alias-adapter/adapter.mjs \
  --root /tmp/fx --alias mypkg --source fx > /tmp/fx/overlay.json
zzop validate-envelope /tmp/fx/overlay.json
```

```
{"valid":true,"issues":[],"hints":[]}
```

The whole envelope is one file entry — this is what a 137-line program produced, and you could have
typed it:

```json
{"format":"zzop-normalized-ast","version":"0.27.0","parser":"python-package-alias-adapter/1",
 "source":"fx","files":[{"path":"train.py","loc":8,"symbols":[],"imports":{
 "base_config":{"specifier":"./core/config","original":"base_config"},
 "helper":{"specifier":"./common/utils","original":"helper"},
 "mypkg":{"specifier":"./common/utils","original":"*"}},"re_exports":[],"dynamic_imports":[],
 "used_names":[],"const_map_fragment":{},"procedure_router_fragments":[],"router_mount_fragments":[],
 "io":{"provides":[],"consumes":[]},
 "overrides":{"imports":["base_config","helper","mypkg"]},"degraded":false,"is_entry":false}]}
```

Wire it in and recount:

```sh
printf '{ "trees": [ { "root": ".", "sourceId": "fx", "overlays": ["./overlay.json"] } ] }\n' \
  > /tmp/fx/zzop.config.jsonc
zzop coverage /tmp/fx
```

```
{... "parserDispatched":6,"resolvedImportEdges":2,"symbols":3}

adapter overlay "fx" (parser python-package-alias-adapter/1) DISPLACED 3 natively-parsed import
binding(s) it declared in `overrides`: train.py 'base_config': ours "mypkg.core.config" -> theirs
"./core/config"; train.py 'helper': ours "mypkg.common.utils" -> theirs "./common/utils"; train.py
'mypkg': ours "mypkg.common.utils" -> theirs "./common/utils". Those native facts are no longer in
this run's output — this line is the only record that they were extracted at all.
```

0 → 2 edges, three displacements, all of them named. That is the loop.

## Worked example B — the same recipe at scale

`twitter/the-algorithm-ml` imports itself as `tml` in a tree where nothing declares that name (the
mechanism is one `ln -s` in a venv build script — see the
[adapter's README](../../examples/adapters/python-package-alias-adapter/) for why no static reader can
find it). Identical three commands, same adapter, `--alias tml`:

| | native only | + overlay |
|---|---|---|
| `resolvedImportEdges` | 3 | **167** |
| `.py` files in the dep graph | 1 | **70** |
| `architecture.pain` | 12.9 | 43.4 |
| `architecture.criticalTop` | empty | 3 files |
| `architecture.topRecommendation` | none | `circular` @ `core/debug_training_loop.py` |

One channel, 188 bindings across 69 of the 91 files, 179 of them displacing a native binding. The
adapter's output is deterministic — regenerated ten days after an earlier local run over the same
checkout, the two files were byte-identical (`cmp`).

**What did not move, and why that is not a failure of injection**: `joinVisibility` stayed at
`provides: 0`. This overlay fills `imports` and nothing else, and the field's own `meaning` says so —
"an overlay carrying only imports or attributes does not" restore join visibility. The right response
is to decide whether you need that channel, not to widen this adapter until the number moves.

---

## Traps, each one measured

**Do not double-produce a router-composition channel another producer already fills.** Fragments from
an overlay are appended to the native fragments for the same file, and `router_mount_fragments` /
`procedure_router_fragments` compose in **one** by-name graph. Two of that graph's rules are per
**file**, and a second producer breaks both:

- an alias mount (`from .api import router as api_router`) finds no fragment of that name in the
  target file, so it resolves to the file's **sole** fragment — a file described twice has none, the
  mount resolves to nothing, and the whole subtree beneath it emits no route;
- a fragment is a root only if no mount **anywhere** names it, and that name set is global, so one
  producer's mounts exclude the other producer's fragments from being walked at all.

Both rules are right on their own — neither can be relaxed without inventing an answer — and root
selection cannot simply be scoped per producer either, because a natively parsed parent mounting an
overlay-supplied child is exactly the composition Mode B is for. So the composition can emit less than
either side alone. Measured with `cargo run --release -p zzop-engine --example fastapi_overlay_adapter
-- <tree>`, which prints its own before/after, on two FastAPI trees the native Python parser already
reads:

```
be-fastapi      BEFORE provides=19  AFTER provides=0
be-fastapi-fs   BEFORE provides=25  AFTER provides=2
```

**The run now says so.** Those numbers are unchanged — the engine still refuses to guess — but the
subtraction is no longer silent. Any overlay describing a file whose fragments another producer had
already described gets one aggregate `warnings` line naming the count, every path, and both sides'
fragment names:

```
adapter overlay "be-fastapi" (parser fastapi-overlay-prototype/1): 9 file(s) already carried
router-composition fragments that this overlay ALSO describes: app/api/routes/api.py (router-mount):
ours [router] + theirs [router]; ... Both descriptions enter ONE by-name composition graph, where a
fragment is a root only if no mount anywhere names it and an alias mount resolves to the target
file's SOLE fragment — a file described twice satisfies neither, so mounts below it resolve to
nothing and their whole subtree emits no route.
```

There is no `overrides` for this channel — `overrides` covers `imports` only — so the engine has no
way to be told which side is authoritative and does not pick one. The fix is yours: drop those files
from the overlay's `files[]`, or emit only the channels the other producer left empty. The
`fastapi_overlay_adapter` example is the reference for what native deliberately does **not** cover —
non-literal `APIRouter` prefixes, other Python web frameworks, per-project conventions — and its
header says so; on a tree where native already extracts routes it is the wrong tool. Check
`census.ioProvides` before you inject into a provide channel, and re-check it after.

**Match `source` to the tree's actual id.** An overlay's facts merge onto whichever tree carried it
regardless of what `source` says; a non-empty mismatch warns that its `io` will join as intra-source.
The default id differs by invocation — a `trees[]` entry with no `sourceId` defaults to its own raw
`root` string, while a bare-path run defaults to the root directory's basename — so set `sourceId`
explicitly and use it.

**Key normalization is byte-exact or it silently does not join.** If your adapter emits `io` for HTTP,
it must reproduce the engine's `"METHOD /path"` normalization exactly; a near-miss produces no error,
just an unjoined consume. [adapters/README.md](../adapters/README.md) is the rule set and
[key-normalization.fixture.json](../adapters/key-normalization.fixture.json) is the pinned table to
test against — string in, string out, no engine build needed.

**`symbols` is not coverage.** The Mode B merge never reads an overlay entry's `symbols`, so an entry
carrying only symbols merges, counts nothing, and leaves the extension's "no native parser" diagnostic
firing. The consumed facts are `io`, `imports`, `re_exports`, `dynamic_imports`, a fragment channel,
`attributes`, and `is_entry`.

---

## Mode A, and when to reach for it

Everything above is Mode B — a partial envelope merged onto a native pass. **Mode A** is a full
envelope that replaces native parsing for a whole tree, run with `zzop analyze-envelope <file>` (the
`zzop` CLI binary, not `zzop-mcp`), the `analyze_envelope` MCP tool, or a direct `zzop-facade`
embedding. Reach for it when there is no native pass worth merging onto — a language with no parser
here at all — and start from `zzop contract example-envelope`, a complete valid sample. The channel
question, the size question and the verification loop are the same; only the merge disappears.
