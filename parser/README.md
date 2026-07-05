# parser/ — parser frontends

Language parsers and schema-DSL parsers (like Prisma) play **the same role** — frontends that take source
and **project a Normalized AST -> Common IR**. So they live together in a single `parser/`, mapping directly
to the diagram's "Parser" layer.

## Contract
Each parser produces a `core::CommonIr` (dep/symbols/loc + optional `IoFacts`). The native types of swc / external
parsers stay **inside** the parser crate and never leak into the engine or rules — see `docs/modules/parsers.md`'s
"Isolation invariant" section (swc version isolation: an swc upgrade should never leak into the public IR).

| Crate | Approach |
|-------|----------|
| `parser-typescript` | **native swc** (parses in Rust, 0 N-API crossings) |
| `parser-prisma` | parses the Prisma schema DSL |
| `parser-java` | external process -> serialized Normalized AST, one crossing |

> A language dispatcher (in `core`) routes files to parsers by extension map + path-glob overrides — supporting a
> single polyglot repo. Because the cross-layer linker is a multi-source join, even a crude JSP parser joins as a
> first-class citizen as long as it extracts accurate IoFacts.

## Languages without an in-workspace crate (JSP, Python, ...)

JSP/Python (and any other language) support does not require its own crate here — it arrives via the
**external-parser envelope protocol** (`docs/NORMALIZED_AST.md`): any out-of-process tool that emits a
`NormalizedEnvelope` JSON document (validated by `zzop_core::validate_envelope`, consumed by
`zzop_engine::analyze_envelope`) plugs in without touching this workspace at all. `docs/examples/jsp-envelope.example.json`
is a worked example of exactly that for JSP. Earlier placeholder crates (`parser-jsp`, `parser-python`) were
removed since they carried no code beyond a stub comment — the envelope protocol supersedes needing an
in-workspace crate per language up front.
