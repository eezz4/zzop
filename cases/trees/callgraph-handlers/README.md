# `callgraph-handlers` — the code half of the `callgraph` fixture, with the routes lifted out

This tree exists for exactly one consumer: `rule_contracts::channel_direction`, the two-arm probe that
MEASURES what an empty io channel does to each rule that reads it. It is `cases/trees/callgraph`'s two
files with their last line — the `apiRoutes.get(...)` / `apiRoutes.put(...)` registration — removed, and
nothing else changed.

## Why a tree had to be split in half

The probe's other hosts are empty directories: a channel is injected as a Mode B adapter overlay and
withheld by simply not attaching it, so the two arms differ by that channel and nothing else. That works
for every rule whose input is io alone. It does NOT work for the call-graph-BFS family
(`mutating-route-no-auth`, `unsafe-read-endpoint`, `non-idempotent-write`), because those rules need a
second input the overlay lane cannot carry:

- an overlay can inject the http PROVIDE (the route), but
- the call edges come from a re-parse of the tree's **own sources** (`analyze::native_rules::callgraph`
  re-reads and re-parses this tree's `.ts` files; `FileArtifact` carries no `RawCall`s), and Mode B
  explicitly does not consume the envelope `calls` channel — it warns and drops it.

So on an empty host those three rules ran over an empty symbol graph and reported nothing in EITHER arm,
which the probe can only publish as `Unobserved` — ignorance, not an all-clear. Splitting the fixture is
what turns that into a measurement: the CODE stays real and identical across both arms (this tree), and
the ROUTE becomes the one thing the probe adds and removes (lifted from `cases/trees/callgraph`, whose
`IoProvide`s carry the handler symbols `statsWithWrite` / `upsertRecord` that the functions here define).

## The two files must keep the names they have

The overlay's projections land on the paths the donor's facts came from (`readEndpoint.ts`,
`writeEndpoint.ts`). Matching names here mean the injected route merges onto this tree's real artifact
instead of being pushed as a synthetic entry — the arrangement the probe's donor doc describes and the
one that mirrors what really happens in a repo (one file, route line and handler together).

## Removing the route line is the point, not a shortcut

The usual fixture discipline is that a fixture on a suppression path must never drop a line the real file
has — the dropped line is exactly the one that makes the defect. This tree is the opposite direction: it
must make the rules FIRE in the supplied arm, and the removed line is the very channel under test. If a
route registration is ever added back here, the withheld arm stops being empty and the probe's floor goes
red, which is the intended alarm.

## If you change this tree

`the_shipped_direction_table_is_exactly_what_the_probe_measures` re-derives the whole direction table on
every `cargo test` and will disagree. A drop of those three rules back to `Unobserved` means the call
graph here stopped reaching the write — fix this tree, do not paste the regression into the table.
A dedicated pin holds that separately, so the regression cannot be pasted away quietly.

The pin lives in `crates/engine/src/channel_direction.rs`
(`the_call_graph_family_stays_measured_on_an_empty_http_provide_channel`).
