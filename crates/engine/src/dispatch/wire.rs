//! The CONFIG-FILE spelling of each [`Language`] — the vocabulary `parsers.globOverrides[].language`
//! accepts.
//!
//! A separate file from `dispatch.rs` since 2026-07-27 (line cap), and the separation happens to say the
//! right thing: [`Language`] itself is an internal enum whose variant names are deliberately NOT a wire
//! format (see its own doc — no `Serialize`/`Deserialize`, so renaming a variant stays cache-safe),
//! while everything here IS the wire. Keeping the mapping hand-written in one place is what lets both
//! statements stay true at once.

use super::Language;

impl Language {
    /// The config-file spelling of this language, for `parsers.globOverrides[].language`.
    ///
    /// Hand-written rather than a `Serialize` derive, deliberately: this type's own doc states that it
    /// derives no `Serialize`/`Deserialize` so renaming a VARIANT stays cache-safe. A derive would tie
    /// the two together and quietly make every variant name a wire name. Keeping the mapping explicit
    /// costs one line per language and preserves the invariant — and lets the wire name differ where it
    /// should: `Java21` is our parser's grammar version, not something a user should have to type, so it
    /// spells `java`.
    ///
    /// The match is exhaustive with no wildcard arm, so a new `Language` variant does not compile until
    /// somebody chooses its config spelling. That is the point.
    pub fn as_wire(self) -> &'static str {
        match self {
            Language::TypeScript => "typescript",
            Language::Prisma => "prisma",
            Language::Java21 => "java",
            Language::Python => "python",
            Language::Rust => "rust",
            Language::Go => "go",
            Language::Sql => "sql",
            Language::CSharp => "csharp",
        }
    }

    /// Parses a config-file language spelling. `None` for anything unknown — the caller turns that into a
    /// warning naming the accepted set (via [`Language::WIRE_NAMES`]) rather than guessing.
    pub fn from_wire(s: &str) -> Option<Self> {
        Self::WIRE_NAMES
            .iter()
            .copied()
            .find(|lang| lang.as_wire() == s)
    }

    /// Every language a config may name, in the order a diagnostic should list them. Derived from the
    /// variants themselves rather than a second hand-kept list.
    pub const WIRE_NAMES: &'static [Language] = &[
        Language::TypeScript,
        Language::Python,
        Language::Java21,
        Language::Rust,
        Language::Go,
        Language::CSharp,
        Language::Sql,
        Language::Prisma,
    ];
}
