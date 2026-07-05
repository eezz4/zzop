//! `zpz-rule-packs` — not a library. This crate exists only to give the DSL rule packs under `dsl/`
//! (`rules/dsl/<pack>/<pack>.json`) a `cargo test`-visible home for their co-located end-to-end tests
//! (`rules/dsl/<pack>/<pack>.rs`, wired as `[[test]]` targets in this crate's `Cargo.toml`) — see
//! `rules/README.md` for the folder layout and `docs/rules/authoring-guide.md` for how to author a pack.
//!
//! The DSL packs themselves are plain JSON data, interpreted at runtime by `zpz_core::load_dsl_packs`
//! (`packages/core/src/pack_loader.rs`) — nothing in this crate parses or evaluates them; that lives in
//! `zpz-core` (loading/schema) and `zpz-engine` (evaluation), both of which this crate's tests depend on
//! as dev-dependencies.
//!
//! Native rules (whole-graph analyses each registered via their own owning crate's
//! `register_native_analyses`, e.g. `cross-layer/duplicate-route`/`zpz_rules_graph`,
//! `schema-structural`/`zpz_rules_schema` — composed by `zpz_engine::register_all_native`; the kernel
//! (`zpz-core`) itself registers none) are NOT packs and do not live here — they are ordinary Rust crates
//! under `rules/native/` (`rules-graph`, `rules-schema`), statically linked into `zpz-engine`.
