# Java imports adapter (Mode B overlay example — the minimal on-ramp)

## What it does

The smallest useful adapter, kept deliberately small to teach the principle: **you do not need a
parser — a partial envelope covering just the missing channel is enough.** It was built against the
v0.16-era lexical Java projector, which returned `imports: None` — a Java tree carried zero
dependency edges natively, and this adapter filled exactly that ONE channel: it reads each `.java`
file's `package`/`import` declaration lines (~90 lines of script, no dependencies beyond
[`adapter-kit`](../adapter-kit/)), resolves intra-tree imports to real file paths, and emits a
[Mode B overlay](../../docs/NORMALIZED_AST.md) envelope whose `FileProjection`s carry only
`path` + `loc` + `imports`. **That Java gap is now closed** — `zzop-parser-java-21` (full CST)
extracts imports natively, so on today's Java trees this overlay is a no-op (parsed facts are never
overridden; see Contract points). The example stays as the reference for the recipe itself, which
applies unchanged to any extension whose native support is missing a channel.

A full native parser costs hours; this costs a coffee break. Start here, add channels (`io`,
fragments, `is_entry`) only when an analysis you care about needs them.

## Run

```sh
# emit the overlay, then attach it via the `overlays` config key / `adapterOverlays` embedder field
node adapter.mjs --root <javaRoot> [--source <treeId>] > overlay.json

# validate loop while iterating (all offline):
#   zzop adapter validate overlay.json        — structural check + lint hints
#   zzop contract envelope-schema         — the JSON Schema, embedded in the binary
#   zzop contract envelope-guide          — the full envelope contract, embedded in the binary

# tests
node --test test/adapter.test.mjs
```

## Contract points

- Channel: dep-graph `imports` ONLY — no `io`, no symbols, no fragments. The merge
  (`crates/engine/src/envelope/merge.rs`) adopts an overlay's `imports` exactly when the native
  artifact has none; parsed facts are never overridden. The native Java parser now populates
  imports itself, so on a `.java` tree this overlay merges as a no-op — the precedence is pinned
  end-to-end by `crates/engine/tests/analyze_java_imports_overlay.rs`.
- Resolution index: fully-qualified class name -> file path, built from each file's OWN `package`
  declaration + its filename (ground truth over directory-convention guessing — any source prefix,
  `src/main/java/` or none, works).
- Import forms: `import a.b.C;` and `import static a.b.C.member;` (and `...C.*`) resolve to class
  `C`'s file. `import a.b.*;` (package wildcard) is skipped — binding it needs body analysis this
  adapter deliberately doesn't do. Unresolvable imports (JDK, external dependencies) are skipped,
  never guessed — only intra-tree edges are the goal.
- Specifiers are emitted **relative to the importing file's directory, keeping the `.java`
  extension** (`../util/TextUtil.java`): the engine's resolver tries the raw join first, so the
  exact target path resolves with no Java-specific extension logic engine-side.
- Envelope: `zzop-normalized-ast` v1, `parser: java-imports-adapter/1`; only a file with at least
  one resolved intra-tree import is projected ([schema](../../docs/adapters/envelope.schema.json)).
  The committed `test/expected-envelope.json` is validated against the REAL
  `zzop_core::validate_envelope` by `crates/engine/tests/analyze_java_imports_overlay.rs`.

## Verified result

Pinned from both sides: the node snapshot test pins the adapter's output bytes, the engine test
pins what the engine does with them. On the committed 3-file fixture the engine test proves the
native Java parser now yields the full dep graph on its own, and that attaching this overlay
changes nothing (merge precedence: an overlay's `imports` are adopted only when the native artifact
has none). In the v0.16 era the same fixture went from zero `.java` dep edges to a real, deduped
`App.java -> TextUtil.java` edge via this overlay — that is the effect to expect on an extension
that still lacks native import extraction.

## Limits (reference-adapter recall, not engine contract limits)

- **Leaf targets don't receive edges.** The engine resolves overlay specifiers against
  fact-carrying files only, and a leaf file that imports nothing intra-tree has no projection — so
  edges INTO it stay unresolved (`TextUtil.java -> Config.java` in the fixture never resolved via
  the overlay; today it resolves, but through the native parser, whose resolution set is every
  parsed file). Files that both import and are imported get full edges.
- **Same-package references are invisible.** Java needs no `import` inside one package; an
  imports-only adapter cannot see those edges.
- **Nested-class imports are skipped.** The FQCN index keys top-level compilation units only
  (`package` + filename), so `import a.b.Outer.Inner;` (and its `import static ...Inner.member`
  form) misses the index and is counted as an external skip — the edge to `Outer.java` is dropped.
  A strip-and-retry loop (`a.b.Outer.Inner` → `a.b.Outer`) would recover it; kept out of the
  reference adapter for line-count honesty.
- Declaration-line regexes: a commented-out `package`/`import` inside a block comment can
  over-match; two same-named classes from different packages in one file cannot happen in
  compiling Java (the collision-suffix key is a formality).
