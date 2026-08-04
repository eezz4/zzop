# python-package-alias-adapter — when the tree is installed under a name no file declares

## What it does

Resolves `import <alias>.x.y` for a Python tree that is importable under a package name **nothing in
the tree states**. Point it at the root, name the alias, get an overlay:

```sh
node adapter.mjs --root <treeRoot> --alias tml [--source <treeId>] > overlay.json
```

Then attach it via the `overlays` key in `zzop.config.jsonc` (embedders: `adapterOverlays`).

**The representative case is now solved natively** — same disclosure shape as
[`../java-imports-adapter/`](../java-imports-adapter/): the `tml` self-import this example was built
against resolves today by declaring the alias in config, `vocabulary.pythonPackageRoots` (e.g. `"tml="`
maps import name `tml` to the tree root), with no overlay involved. The example stays as the reference
for the Mode-B overlay recipe itself — the recipe applies unchanged to an alias mechanism the config
key cannot state.

## Why this is layer 3, and not a parser bug

`twitter/the-algorithm-ml` imports itself as `tml` in 170 places across 70 files. Nothing in the tree
says so:

- `pyproject.toml` carries only `[tool.black]`
- there is no `setup.py`, no `setup.cfg`
- the real mechanism is the last line of `images/init_venv.sh`:

```sh
ln -s "$(pwd)" "$VENV_PATH/lib/python3.10/site-packages/tml"
```

A symlink created at venv-build time, hardcoded to `/opt/ee/python/3.10`, Linux-only. No static reader
can find that, and "the repo root is probably importable under its own directory name" is a guess the
engine declines to make. The user knows the alias; telling zzop costs one flag. That is what injection
is for.

## It has to DISPLACE, and measuring is what proved it

The plan was that additive merging would be enough — native leaves a dead `tml.*` binding, the adapter
adds one that resolves, both coexist. **That was wrong**, and wrong in the easy-to-miss way: both sides
use the SAME LOCAL NAME. `from tml.common.filesystem import infer_fs` binds `infer_fs` natively (to the
unresolvable `tml.common.filesystem`) and this adapter binds `infer_fs` too. That is a collision, native
wins by default, and the dead binding stays.

Measured on the real tree: **179 of 188 offered bindings were dropped that way**, and the run said so —
the drop disclosure is what diagnosed it. So each projection declares the local names it replaces, which
puts the envelope at the `0.27.0` floor. See `docs/NORMALIZED_AST.md`'s `overrides` field.

## Verified result (`twitter/the-algorithm-ml`, 91 Python files)

| | native only | + this overlay |
|---|---|---|
| import edges | **3** | **167** |
| findings | 0 | 1 (`circular`) |
| `criticalTop` | empty | 3 files |
| `topRecommendation` | none | `core/debug_training_loop.py` |
| `pain` | 12.9 | 43.4 |

The three native edges are the tree's only relative imports; everything else went through the alias and
was invisible. With the overlay the tree gets the same channels a natively-parsed tree gets: a real dep
graph, cycle detection that finds a real cycle, criticality ranking, and a recommendation.

The run also prints one displacement line naming all 179 replaced bindings with both specifiers. It is
long on purpose — each entry is a native fact this adapter asserted was wrong, and nothing verifies that
the adapter is the correct side.

**Not shown, and not a failure of injection**: seams and bus-factor stay empty because those read git
history and the corpus is a `--depth 1` clone (one commit). Clone with history to exercise them.

## Contract points

- Channel: dep-graph `imports` only — no `io`, no symbols, no fragments.
- Local names follow **CPython's binding rules**, not the specifier text: `from a.b import c` binds `c`,
  `import a.b.c` binds only `a`, `as` overrides both. This is not cosmetic — the engine keys `imports`
  by local name, so a binding under the wrong key arrives as a sibling entry and displaces nothing (see
  [`../override-required/`](../override-required/), the fixture built around exactly that failure).
- Specifiers are emitted **relative to the importing file** and left for the engine's own Python
  resolver to try against the paths that exist. A candidate is a question, not a claim: an alias segment
  naming nothing simply stays unresolved, as it does without the overlay.
- Star imports (`from a.b import *`) are skipped — they bind no single name this adapter can key.
- Line-based extraction. Both statement forms put the whole dotted name on one line in real code; this
  adapter resolves module paths, not expressions.

## Tests

```sh
node --test test/adapter.test.mjs
```
