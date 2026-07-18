# parser/ — parser frontends

Language parsers and schema-DSL parsers (like Prisma) play **the same role** — frontends that take source
and **project a Normalized AST -> Common IR**. So they live together in a single `parser/`, mapping directly
to the diagram's "Parser" layer.

## Contract
Each parser produces a `core::CommonIr` (dep/symbols/loc + optional `IoFacts`). The native types of swc / external
parsers stay **inside** the parser crate and never leak into the engine or rules — see
[`docs/ARCHITECTURE.md`](../docs/ARCHITECTURE.md)'s "The IR your `ir` field contains" section (an swc upgrade
should never leak into the public IR).

| Crate | Approach |
|-------|----------|
| `parser-typescript` | **native swc** (parses in Rust, 0 N-API crossings) — full AST |
| `parser-python-3` | **native ruff** (Astral's published parser crates — the same grammar powering ruff/ty, in Rust, 0 process crossings) — full AST |
| `parser-rust` | **native syn 2** (parses in Rust, 0 process crossings) — full AST: symbols, imports/dep graph (module-path + workspace `Cargo.toml` resolution), axum router provides, `reqwest` egress consumes |
| `parser-go` | **native tree-sitter-go** (tree-sitter-go 0.25) — full CST: symbols, imports/dep graph (`go.mod` package-directory resolution), gin/`net/http` router provides, `net/http` egress consumes |
| `parser-prisma` | parses the Prisma schema DSL — lexical: model `db-table` provides (accessor-cased key, feeds the db-table join channel) |
| `parser-java-21` | **native tree-sitter-java** (tree-sitter-java 0.23.5, Java 21 grammar coverage) — full CST: symbols, imports/dep graph, Spring MVC HTTP route provides |
| `parser-sql` | parses SQL DDL — lexical/regex: `CREATE TABLE` → `db-table` provides (quote-stripped, schema-qualifier dropped, lower-first canonical key — twin of the Prisma provide, for ORM-less migration stacks) |

> A language dispatcher (in `core`) routes files to parsers by extension map + path-glob overrides — supporting a
> single polyglot repo. Because the cross-layer linker is a multi-source join, even a crude JSP parser joins as a
> first-class citizen as long as it extracts accurate IoFacts.

## The envelope path remains the default for the long tail (JSP, Ruby, ...)

A new language does not get its own crate here by default — it arrives via the **external-parser
envelope protocol** (`docs/NORMALIZED_AST.md`): any out-of-process tool that emits a `NormalizedEnvelope`
JSON document (validated by `zzop_core::validate_envelope`, consumed by `zzop_engine::analyze_envelope`)
plugs in without touching this workspace at all. `examples/jsp-envelope.example.json` is a worked example
of exactly that for JSP.

Promotion out of the envelope path happens on the **commonality criterion** that governs zzop's native
middleware recognizers (see `docs/ARCHITECTURE.md`'s "Language support" section): a language common
enough in real polyglot backends earns full-AST/CST treatment in-workspace rather than being left to an
adapter. Python set the precedent (promoted from the envelope path to `parser-python-3`, ruff-based);
Rust (`parser-rust`, syn 2) and Go (`parser-go`, tree-sitter-go) have since cleared the same bar; SQL
DDL (`parser-sql`, lexical) most recently joined as a `db-table` provider for ORM-less migration stacks.
The table above is the current native list. A native crate's v1 scope can still be deliberately narrower
than what an adapter covers: Python's is (Flask/Django routes, `Depends` auth attributes, ORM table
facts, ... — see `crates/engine/examples/fastapi_overlay_adapter/main.rs` for the Mode-B overlay that
still covers those shapes). Any other language stays on the envelope path unless it clears the same
commonality bar.
