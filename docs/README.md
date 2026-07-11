# zzop documentation map

| Page                                                 | Answers                                                                                                                                                      |
| ----------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [getting-started.md](getting-started.md)             | How do I install and run zzop on my repo, read the output, and silence a false positive?                                                                    |
| [ARCHITECTURE.md](ARCHITECTURE.md)                   | How does a call to `analyze`/`analyzeTrees` process my tree — what's in the `ir` output, what does `degraded` mean, how does caching behave, what languages does the engine understand natively?  |
| [NORMALIZED_AST.md](NORMALIZED_AST.md)               | I'm writing an external/custom parser adapter (Java, Python, JSP, ...) — what JSON envelope must it emit to join the analysis as a first-class source?      |
| [../examples/](../examples/README.md)                | What does a real Normalized AST envelope look like end-to-end, and how do I extend zzop? A full JSP envelope sample (Mode A) plus two Mode-B adapter examples ([openapi-sdk-adapter/](../examples/openapi-sdk-adapter/), [svelte-adapter/](../examples/svelte-adapter/)). |
| [modules/napi.md](modules/napi.md)                   | How do I call the engine — the four functions (`analyze`/`analyzeTrees`/`analyzeEnvelope`/`version`), request/response JSON shapes, error handling, npm packaging. |
| [rules/dsl-reference.md](rules/dsl-reference.md)     | What is the exact JSON schema for a `rules/dsl/*.json` pack — every matcher field, suppress-marker semantics, schema-version policy, finding shape?         |
| [rules/authoring-guide.md](rules/authoring-guide.md) | How do I write and ship a new DSL rule pack — placement, a worked example, performance pitfalls, the testing/fidelity bar, and when to write a native rule instead? |
| [rules/catalog.md](rules/catalog.md)                 | What rules ship today — every DSL pack's rules and every native analysis id, with what each one detects?                                                    |
| [adapters/README.md](adapters/README.md)             | I'm writing a non-Rust adapter — how do I prove my HTTP interface-key normalization is byte-identical to the engine's, so cross-layer joins don't silently fail? |
