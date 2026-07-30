# override-required — the tree where ADDING facts cannot fix the graph

Every other adapter example here demonstrates something an adapter can do by CONTRIBUTING. This one is
the committed measurement of the case that needs the opposite: **displacing a native fact that is
wrong.** It was built as the acceptance fixture for the partial-overriding design and is now that
design's e2e test (`crates/engine/tests/analyze_override_displacement.rs`).

## The shape

```
util/config.py          <- stale vendored copy at the tree root ("stale")
src/util/config.py      <- the real module, src-layout ("live")
src/app.py              <- import util.config
```

`import util.config` resolves through path candidates tried in order, root first. Both spellings name a
real file, so the ROOT one wins: the native parser emits `src/app.py -> util/config.py`. Resolvable,
deterministic — and wrong. The import means the src-layout module.

This is not an exotic corner. Any tree that vendors a stale copy of a package it also develops, or keeps
a build artifact beside its source, has this shape: two real files answering one name, first match wrong.

## Why a declaration is required, and not just "the overlay wins"

The first attempt at this fixture keyed the adapter's binding `util.config` — the specifier, which reads
like the obvious key. It did not collide with anything: **Python binds `import util.config` under the
local name `util`**, so the correction arrived as a sibling entry and both edges survived.

That measurement is why `overrides` is a declaration rather than key priority. The displacing side often
does not know it is colliding, so it has to NAME what it displaces — and naming it in the engine's key
space (local names) is part of getting it right.

## Measured

```
native alone                        src/app.py -> util/config.py          (1 edge, wrong)

+ overlay, no override declared     src/app.py -> util/config.py          (parsed fact wins)
                                    warning: ... binding(s) it offered were DROPPED ...

+ overlay with overrides.imports    src/app.py -> src/util/config.py      (1 edge, correct)
                                    warning: ... DISPLACED 1 natively-parsed import binding(s) ...
                                             src/app.py 'util': ours "util.config"
                                                              -> theirs "src.util.config"
```

Both directions of loss are reported. The declared override displaces the native binding and the run
says so, naming both sides — that line is the only record the native fact was ever extracted. Without a
declaration the parsed fact wins (the correct default) and the run says the adapter's side was dropped,
so a misspelled or forgotten declaration cannot look like success.

## Run it

```sh
zzop graph --domain dep --format cosmograph-links --config examples/adapters/override-required/zzop.config.jsonc
zzop analyze examples/adapters/override-required          # the warnings channel carries the disclosure
```

Remove the `overlays` key from `zzop.config.jsonc` for the native-only baseline; delete the `overrides`
block from `overlay.json` (and drop `version` back to 1) for the undeclared-collision case.

## What is still open

Deletion is not offered and is refused at the contract boundary: an `overrides` entry with no
replacement binding fails validation. There is no way for an adapter to make the engine forget a fact
and put nothing in its place — it has no honest output form, and an adapter that can delete can blind
the engine silently.

Nothing verifies that the adapter is the correct side. The disclosure exists so a reader can disagree.
