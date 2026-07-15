//! `zzop-rule-packs` — not a library. This crate exists only to give the DSL rule packs under `dsl/`
//! (`rules/dsl/<pack>/<pack>.json`) a `cargo test`-visible home for their co-located end-to-end tests
//! (`rules/dsl/<pack>/<pack>.rs`, wired as `[[test]]` targets in this crate's `Cargo.toml`) — see
//! `rules/README.md` for the folder layout and `docs/rules/authoring-guide.md` for how to author a pack.
//!
//! The DSL packs themselves are plain JSON data, interpreted at runtime by `zzop_core::load_dsl_packs`
//! (`crates/core/src/pack_loader.rs`) — nothing in this crate parses or evaluates them; that lives in
//! `zzop-core` (loading/schema) and `zzop-engine` (evaluation), both of which this crate's tests depend on
//! as dev-dependencies.
//!
//! Native rules (whole-graph analyses each registered via their own owning crate's
//! `register_native_analyses`, e.g. `circular`/`zzop_rules_graph`, `duplicate-route`/`zzop_rules_http`,
//! `cross-layer/duplicate-route`/`zzop_rules_cross_layer`, `schema-structural`/`zzop_rules_schema` —
//! composed by `zzop_engine::register_all_native`; the kernel (`zzop-core`) itself registers none) are NOT
//! packs and do not live here — they are ordinary Rust crates under `rules/native/` (`rules-graph`,
//! `rules-http`, `rules-cross-layer`, `rules-schema`), statically linked into `zzop-engine`.
