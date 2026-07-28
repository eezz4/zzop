# examples/adapters — the one extension path that needs a worked example

Runnable references for the two authoring modes, both speaking the Normalized-AST contract
([`docs/NORMALIZED_AST.md`](../../docs/NORMALIZED_AST.md); authoring guide
[`docs/adapters/README.md`](../../docs/adapters/README.md)). Each is a reference, not a product
dependency — copy it, point it at your repo. Approach, usage and limitations live in each folder's
own README.

**Why only three.** This directory held nine adapters until 2026-07-28. Six of them
(`openapi-sdk`, `oazapfts`, `react-query`, `svelte`, `wrapper`, `rust-parser`) demonstrated the same
contract over a different framework's idioms, and that is the long tail this engine has already
decided it cannot win by enumeration — chasing it in examples is the same losing race run in
documentation instead of code. What survives covers the contract itself: the shared library, one
Mode B overlay, one minimal channel-filling adapter. A framework flavor is not a new thing to learn;
a new *channel* is.

⚠ **`adapter-kit` mirrors engine logic, and the mirror is now enforced.** Its key normalization must
stay byte-identical to `zzop_core::io::key` — that module's own header says so ("their logic must not
change here without changing every mirror"). Nothing checked it until 2026-07-28: a Rust-side change
could silently make every copied adapter emit mis-keyed envelopes, and a mis-keyed envelope joins
wrongly and quietly, which is the failure this repo treats as worst. CI now runs the kit's tests, so
a divergence goes red here instead of being discovered in somebody's join.

## Mode A — bundle (full envelope for a new language, replaces native analysis via `analyzeEnvelope`)

The minimal envelope example moved to
[`docs/contracts/example-envelope.json`](../../docs/contracts/example-envelope.json) on 2026-07-28:
it is not something anybody copies, it is a **shipped contract document** (`zzop contract
example-envelope`, and an MCP resource), and every one of its siblings already lived under
`docs/contracts/`. It sat here long enough that its six `include_str!` consumers were a surprise.

## Mode B — overlay (partial envelope merged onto native analysis via `adapterOverlays`)

- [`java-imports-adapter/`](java-imports-adapter/) — **start here**: the minimal on-ramp, one
  channel (`imports`) in ~90 lines — built when the v0.16-era lexical Java projector left a
  dep-graph gap (`imports: None`), proving a partial envelope is enough; no parser required. The
  native Java parser has since closed that gap, so on Java trees it now merges as a no-op — kept as
  the reference recipe for any extension still missing a channel, and its
  `test/expected-envelope.json` is a live fixture for `analyze_java_imports_overlay`.
- [`auth-overlay-adapter/`](auth-overlay-adapter/) — a demo of the entity-ATTRIBUTES injection
  channel: router-level `app.use('/prefix', requireAuth)` guards injected as file attributes to
  close `mutating-route-no-auth`'s middleware blind spot for non-Express frameworks/custom
  middleware naming — common Express guard registrations are now recognized natively (see the
  rule's catalog entry).

## Shared

- [`adapter-kit/`](adapter-kit/) — the walk / envelope-builder / key-normalization library the JS
  adapters import (key normalization byte-identical to `zzop_core`).
