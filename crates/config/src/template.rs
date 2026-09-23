//! The starter `zzop.config.jsonc` document — the ONE canon behind three surfaces: the
//! `config-template` embedded contract resource, which `zzop contract config-template` prints and MCP
//! `resources/read` serves (both wired in `zzop_summary::contracts`), and `zzop init`, which is
//! nothing but argv parsing plus a file write of these exact bytes. It lives HERE, in the config front
//! end, for the same reason `config-surface.json` does: the crate that decides what a key MEANS owns
//! the text that teaches it, and the contract table references it rather than embedding a second copy.
//!
//! Two properties are machine-pinned in `template_tests.rs` instead of left to review, because this
//! repo has already paid for both defect classes:
//! 1. **Every key it names is real.** A starter file advertising a key no surface consumes is a new
//!    instance of exactly the defect `warnings::RETIRED_KEYS` had to clean up. The pins run the ACTIVE
//!    keys through this crate's own unknown-key walk, and check the keys named in the COMMENTS against
//!    the same `config-surface.json` vocabulary.
//! 2. **Its values are zzop's own suggestions, not a second set.** Every value below is read from the
//!    symbol its consumer already owns (`VocabularyConfig::built_in`), so the starter file DOCUMENTS the
//!    suggestions instead of quietly re-deciding them.
//! 3. **Its "TypeScript/JavaScript files only" sentences stay true.** Added 2026-08-13 for the third
//!    defect class this file has now paid for: a knob whose consumer is one-language-gated, written into
//!    a Java/Go/Python/C# user's own repository with no sign of that. The pin reads the consuming rules'
//!    OWN sightline declarations rather than a hand list, and it covers four of the eleven such keys —
//!    the rest have no declaration to bind to, and the pin's doc names them instead of implying cover.
//!
//! This property used to be spelled "writing it changes nothing — every value below is the one a run
//! already uses with no config file at all", and the prose in the template said the same to the user.
//! Both went false on 2026-07-27 and neither noticed: config became mandatory (there is no run without
//! a file), and an undeclared vocabulary key became a judgment NOT MADE (so "delete what you do not
//! need" told the reader to switch analyses off while sounding like tidying). D15's sweep of "zzop runs
//! without config" claims covered `docs/`, `site/`, the READMEs and the MCP tool descriptions — and
//! missed THIS file, the one the product writes into the user's own repository, i.e. the most-read
//! config document zzop has. The pins below could not catch it either: they check that every key named
//! is real, never that the surrounding sentences are true. A guard over vocabulary is not a guard over
//! claims.
//!
//! Comment style, and the reason for it: the prose says what a key MEANS and stops there — no counts,
//! no inventories, no "currently". A comment stating today's state rots, and this one rots inside a
//! file the user owns and zzop will never rewrite. Keys are named in backticks so a machine can check
//! them; everything else stays plain prose.

/// The starter `zzop.config.jsonc` bytes, verbatim.
// The document itself lives in `config-template.jsonc` beside this file rather than in a raw string
// here. Two reasons, one of them measured: this module hit the 300-line cap the moment a `rules`
// default was added to the document, and a starter JSONC document held as a Rust string literal is
// the one form of it nothing can lint, diff, or open as what it is. `include_str!` embeds the exact
// bytes at compile time, so every property pinned in `template_tests.rs` is unchanged -- and the
// three shell guards that read the DOCUMENT as text (check-committed-config-vocabulary,
// check-convention-vocab-declarable, check-doc-config-keys) were repointed at the new path in the
// same commit. All three carry a non-emptiness floor, so a stale path there fails loudly.
pub const CONFIG_TEMPLATE_JSONC: &str = include_str!("config-template.jsonc");
