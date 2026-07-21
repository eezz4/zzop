//! Tests for the `${NAME}` DSL rule-pack fragment mechanism (`def::RulePackDef::expand_fragments`,
//! `fragments::shared_fragments`/`fragment_ref_name`). Split into two submodules purely to stay under the
//! repo's per-file line cap:
//! - `expansion_tests` — synthetic-pack unit coverage of the resolution/error contract (`fragment_ref_name`,
//!   shared-vs-per-pack precedence, unknown/nested errors, idempotency, every-field coverage);
//! - `byte_identity` — the real-`rules/dsl`-tree guards: the sentinel-collision check every shipped pack
//!   must pass, that the tree loads with zero errors, and the `Debug`-unchanged byte-identity proof for
//!   two non-`sql` migrated packs.

mod byte_identity;
mod expansion_tests;
