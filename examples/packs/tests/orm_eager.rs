//! `[[test]]` target for the EXPORTED `examples/packs/orm-eager.json` pack.
//!
//! Exists so an exported pack does not lose its tests. The `typescript` export (2026-08-11) deleted
//! 1,392 lines of co-located tests with the pack, which made the exported rules untested the moment
//! they left the bundle — a rule a user can still load and run, with nothing checking it. The three
//! modules below are the ORIGINAL per-rule tests, moved verbatim and repointed at the new pack id.
//!
//! Wired from `rules/Cargo.toml` like every bundled pack; the only difference is the path it loads
//! (`../examples/packs` instead of `dsl`), because `CARGO_MANIFEST_DIR` is still `rules/`.

mod eager_relation_declared;
mod jpa_eager_fetch;
mod sqlalchemy_eager_relationship;
