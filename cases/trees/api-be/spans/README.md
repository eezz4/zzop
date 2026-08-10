# `spans/` — fixtures for the method-scan SPAN-BOUNDARY axis

Every module here probes the same thing, and it is not the rule's own logic: it is the size of the
span a `Matcher::MethodScan` rule pairs its patterns across. `scripts/measure/classify-method-scan-spans.mjs`
sizes that axis (A-exposed = 2+ `patterns` with no `after_in_same_function`, C-veto-only = an `absent`
veto over the same span); these modules are the hand-written half — a rule that is silent on the OSS
corpus is not thereby correct, it is unmeasured, and a fixture is how it gets measured.

## Why a TypeScript class with only PROPERTIES

Until 2026-08-09, `parser/parser-typescript/src/symbol_shapes.rs` emitted a sub-symbol for a class's
constructor, methods and private methods, and skipped everything else — so a class whose members are
all property-assigned arrow functions projected exactly ONE span, the whole class body, and
`crates/core/src/dsl/method_scan/gates.rs::drop_outer_spans` kept it (it only discards an outer span
when a nested candidate exists). That was the shape of the axis's first confirmed false positive —
`egress/get-and-body` on a Vue service class whose `method: "GET"` paired with a `body:` 47 lines
away under `method: "POST"` — and of the 11 FPs and 2 FNs these modules then confirmed.

**The repair landed the day after these fixtures did** (`emit_class` now gives function-valued
properties their own leaf spans). The modules stay, with their roles inverted: every FP probe is now
a permanent regression tripwire — it must stay SILENT, and if the class-wide span ever returns it
fires as an unexpected finding and the detection gate goes red. The two FN probes flipped to ordinary
expectations (the sibling-member veto that silenced them died with the span).

## What each module contains

* **FP probe** — a class as above, with pattern-1 in one property and pattern-2 in a DIFFERENT one, so
  that pairing them is wrong. Each property is innocent read on its own; the code is deliberately
  ordinary service code, not a defect dressed up.
* **TP control** — a standalone `export function`, which DOES get its own leaf span, holding the same
  two patterns genuinely together. Without it a silent probe proves nothing: the rule might be quiet
  for an unrelated reason (a `require_file` gate, a mis-typed pattern, a pack disabled). The control
  is the non-zero that makes a zero mean something.
* For a **C-veto-only** rule the directions swap: the probe is a real defect in one property with the
  `absent`-veto token in a DIFFERENT property, and silence there is a false NEGATIVE.

`cases/EXPECTED.jsonc` records which is which on every entry, because a later projection repair needs
to know which lines are SUPPOSED to flip.

## Java lives elsewhere

The Java arm of the same axis is `trees/java-svc/src/main/java/com/example/svc/LambdaRoutes.java`.
Java methods do get their own spans, so the oversized-span shape there is two sibling handler LAMBDAS
inside one registration method — `parser/parser-java-21/src/lang/calls.rs` states the containment
("a lambda body simply falls within its enclosing METHOD's own span"). It is a weaker claim than the
TypeScript one and that file says why.
