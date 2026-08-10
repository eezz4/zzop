//! `zzop-parser-java-21` — a `tree-sitter-java`-based Java parser frontend -> Common IR projection,
//! mirroring `zzop-parser-go`'s tree-sitter discipline exactly (grammar AST types stay inside this
//! crate; only `zzop_core` types cross the crate boundary — enforced by
//! `scripts/check-tree-sitter-isolation.sh`'s allowlist).
//!
//! ## 2-layer layout
//! - `lang` — CST -> Common-IR LANGUAGE projection: `SourceSymbol` extraction (`symbols`, including
//!   method/constructor BODY SPANS), `ImportMap` extraction (`imports`), identifier-reference
//!   collection (`used_names`), and same-file call-site extraction (`calls`, `RawCall`s feeding the
//!   whole-repo call-graph `SymbolGraph`).
//! - the crate-root sibling modules — framework-vocabulary producers emitting cross-layer IO facts
//!   (route PROVIDES both per-file and over the whole corpus, literal HTTP egress CONSUMES, ORM
//!   `db-table` facts, and AUTH-GUARD evidence feeding the framework-neutral `auth-guarded` channel).
//!   WHICH frameworks and WHICH channels is deliberately not restated here: [`FRAMEWORK_RECOGNIZERS`]
//!   below is the machine-verified answer (aggregated by `zzop_engine::framework_recognizers`, checked by the
//!   engine's `rule_contracts::recognizer_channels` test). A prose copy is a second count that nothing
//!   verifies, and this one had already drifted — it named four producers and went silent on a whole
//!   channel whose two modules sit in the `pub mod` block below. WHICH channel is recorded once, in
//!   `scripts/check-framework-prose-enumeration.sh`; naming it here would put the name back in the
//!   paragraph that must not carry names.
//!
//! ## Tree-sitter discipline (mirrors `zzop_parser_go`'s crate-root doc verbatim — see that crate for
//! the fuller rationale; summarized here)
//! - **Parse once per public fn call.** Every `pub fn` parses `text` exactly once via [`parse_tree`],
//!   then walks the resulting `tree_sitter::Tree`. Sibling public fns each parse independently — calling
//!   `parse_java` and then `extract_http_provides` on the same `text` parses twice total.
//! - **Never-guess on parse errors.** [`parse_tree`] returns `None` when the root is hopeless (crate
//!   root gate below); a PARTIAL error elsewhere never blanks the rest of an otherwise-valid file — every
//!   walk in this crate skips just the erroring subtree via `util::valid_named_children`.
//! - **Node-kind vocabulary is pinned** — `node_kinds::PINNED_NODE_KINDS` (test-only), asserted against
//!   the compiled `tree_sitter_java::LANGUAGE`.
//! - **No tree-sitter types in the public API.**

pub mod http_clients;
pub mod jpa;
pub mod lang;
pub mod project;
pub mod provides;
pub mod security;
pub mod spring_security;
mod util;

use zzop_core::recognizer::{channel, FrameworkRecognizer};

/// Frameworks this parser recognizes — see [`zzop_core::recognizer`].
///
/// The consume side was EMPTY until 2026-08-02 (the declaration here said so out loud: a Java service
/// that CALLS another service contributed nothing to the cross-layer join). `http_clients` now fills it
/// for `RestTemplate`/`WebClient`, and `jpa` adds the db channel. Still absent, still disclosed: Feign
/// (`@FeignClient` declarative interfaces) and `java.net.http.HttpClient` (URL not visible at the
/// `send` call site) — see `http_clients`'s module doc for the reasons each is roadmap rather than
/// recognized.
pub const FRAMEWORK_RECOGNIZERS: &[FrameworkRecognizer] = &[
    FrameworkRecognizer {
        framework: "spring",
        extensions: &["java"],
        emits: &[channel::PROVIDES],
    },
    // Spring Security emits NO io: `security` (method-security annotations) and `spring_security`
    // (global posture) both feed the decorator-guard side channel that EXEMPTS routes from
    // `mutating-route-no-auth`. This row declared `io.provides` until 2026-08-02 — a false claim the
    // channel contract caught and pinned; `evidence.auth-guarded` is the channel that let it state
    // the truth (`emits` must be non-empty, and none of the io three was it).
    FrameworkRecognizer {
        framework: "spring security",
        extensions: &["java"],
        emits: &[channel::AUTH_EVIDENCE],
    },
    FrameworkRecognizer {
        framework: "resttemplate",
        extensions: &["java"],
        emits: &[channel::CONSUMES],
    },
    FrameworkRecognizer {
        framework: "webclient",
        extensions: &["java"],
        emits: &[channel::CONSUMES],
    },
    FrameworkRecognizer {
        framework: "jpa",
        extensions: &["java"],
        emits: &[channel::DB],
    },
];

#[cfg(test)]
mod node_kinds;

pub use http_clients::extract_java_http_consumes;
pub use jpa::extract_jpa_db_table_provides;
pub use lang::call_sites::extract_call_sites;
pub use lang::calls::parse_calls;
pub use lang::imports::parse_imports;
pub use lang::loop_spans::extract_loop_spans;
pub use lang::string_literals::extract_string_literals;
pub use lang::symbols::parse_symbols;
pub use lang::used_names::parse_local_identifier_refs;
pub use project::{extract_http_provides_project, ProjectProvidesReport};
pub use provides::extract_http_provides;
pub use security::extract_spring_guarded_lines;
pub use spring_security::{
    extract_spring_security_posture, SpringAntMatcher, SpringSecurityPosture,
};

/// Cache-bust token for `zzop-cache`: `parser-id/pinned-toolchain/last-change-version`. The
/// `tree-sitter-java` segment names this crate's REAL exact pin (`tree-sitter-java = "=0.23.5"` in
/// `Cargo.toml`), so it stays accurate for whoever reads it — unlike
/// `zzop_parser_typescript`'s caret-range label. Keeping the two in step is a courtesy to that
/// reader, not a correctness duty.
///
/// **This string is an ID, not a version — it no longer has to be bumped.** `crates/engine/build.rs`
/// hashes this crate's whole dependency closure into the cache key beside it, so a change to any
/// source here invalidates on its own. What is left is the part a person reads in a cache path or a
/// bug report: which frontend parsed the file. Change it when the FRONTEND changes; correctness no
/// longer depends on remembering.
pub const PARSER_FINGERPRINT: &str = "java21/tree-sitter-java-0.23.5/0.21.0";

/// Every top-level declaration kind this crate recognizes, PLUS `module_declaration` (never itself
/// extracted, but still a sign the file has SOME real Java in it) — the root-hopeless gate's "is there
/// at least one of these among the root's own top-level children?" set. Mirrors
/// `zzop_parser_go::TOP_LEVEL_DECLARATION_KINDS`'s exact role and doc.
const TOP_LEVEL_DECLARATION_KINDS: &[&str] = &[
    "package_declaration",
    "import_declaration",
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "record_declaration",
    "annotation_type_declaration",
    "module_declaration",
];

/// Parses `text` with `tree-sitter-java`, returning `None` when the root "fails to parse" — either
/// `Node::is_error()` on the root directly, or (the far more common real-world signal, mirroring
/// `zzop_parser_go::parse_tree`'s identical two-gate shape) when NONE of the root's own top-level
/// children survive as a recognized, non-error/non-missing declaration kind
/// ([`TOP_LEVEL_DECLARATION_KINDS`]). A file with at least ONE valid top-level declaration alongside
/// broken ones still returns `Some` — a partial error elsewhere must not blank out an otherwise-fine
/// file.
///
/// Known parity deviation (deliberate, mirrors `zzop_parser_go`'s own documented F4 comment-only-file
/// gap): a COMMENT-ONLY `.java` file hits the second gate (its named children are all
/// `line_comment`/`block_comment`, none a declaration) and is reported degraded, whereas TS/Python/Rust
/// do not degrade a comment-only file. Accepted for the same reason Go accepts it: the only observable
/// difference is the `degraded` flag (such a file carries no symbols/imports either way), and an EMPTY
/// file (zero named children) short-circuits the `> 0` guard and is NOT degraded, matching the sibling
/// parsers. Internal-only: `tree_sitter::Tree` never crosses this crate's public API.
pub(crate) fn parse_tree(text: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&java_language()).ok()?;
    let tree = parser.parse(text, None)?;
    let root = tree.root_node();
    if root.is_error() {
        return None;
    }
    if root.named_child_count() > 0
        && !util::valid_named_children(root)
            .iter()
            .any(|c| TOP_LEVEL_DECLARATION_KINDS.contains(&c.kind()))
    {
        return None;
    }
    Some(tree)
}

fn java_language() -> tree_sitter::Language {
    tree_sitter_java::LANGUAGE.into()
}

/// Raw physical line count — mirrors every other parser crate's `count_loc` exactly. The file is never
/// parsed here, so this is safe to call even when [`parse_tree`] would return `None`.
pub fn count_loc(text: &str) -> u32 {
    text.split('\n').count() as u32
}

/// Language projection: source -> `(symbols, imports, loc, used_names)`, the tuple mirroring
/// `zzop_parser_go::parse_go`'s pipeline slot shape. Returns `None` when `parse_tree` fails on `text` —
/// what the caller does with that is the caller's business (today
/// `zzop_engine::pipeline::parsers::parse_java21` flags the file `degraded` and contributes no symbols
/// or imports for it; there is no second Java frontend to fall back to).
pub fn parse_java(
    rel: &str,
    text: &str,
) -> Option<(
    Vec<zzop_core::SourceSymbol>,
    zzop_core::ImportMap,
    u32,
    Vec<String>,
)> {
    parse_tree(text)?; // parse-failure gate only — each sub-call below re-parses independently.
    let symbols = lang::symbols::parse_symbols(rel, text);
    let imports = lang::imports::parse_imports(text);
    let loc = count_loc(text);
    let used_names: Vec<String> = lang::used_names::parse_local_identifier_refs(text)
        .into_iter()
        .collect();
    Some((symbols, imports, loc, used_names))
}

/// This file's `package a.b.c;` declaration, dotted, verbatim — `None` when absent (the default
/// package, or a parse failure). The engine builds a `(package, type)` -> file index from this and
/// [`java_type_names`] for cross-file import resolution (`zzop_engine::pipeline::java_index`). Note
/// this is a SHARPER key than `project`'s own resolution uses — that pass matches classes by SIMPLE
/// NAME only and counts the ambiguity instead (`ProjectProvidesReport::skipped_ambiguous_class_name`).
pub fn java_package_of(text: &str) -> Option<String> {
    let tree = parse_tree(text)?;
    for child in util::valid_named_children(tree.root_node()) {
        if child.kind() != "package_declaration" {
            continue;
        }
        for name in util::valid_named_children(child) {
            if matches!(name.kind(), "identifier" | "scoped_identifier") {
                return Some(util::node_text(name, text).to_string());
            }
        }
    }
    None
}

/// Every top-level type name declared in this file (class/interface/enum/record/annotation-type) —
/// simple names, NOT dotted with the package. Nested types are excluded (a nested type's binary name
/// `Outer.Inner` is not a separately importable top-level compilation unit). Empty on parse failure or
/// a file declaring no top-level type at all (a package-info.java, for instance).
pub fn java_type_names(text: &str) -> Vec<String> {
    let Some(tree) = parse_tree(text) else {
        return Vec::new();
    };
    util::valid_named_children(tree.root_node())
        .into_iter()
        .filter_map(|child| {
            matches!(
                child.kind(),
                "class_declaration"
                    | "interface_declaration"
                    | "enum_declaration"
                    | "record_declaration"
                    | "annotation_type_declaration"
            )
            .then(|| child.child_by_field_name("name"))
            .flatten()
            .map(|n| util::node_text(n, text).to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_java_returns_none_on_hopeless_input() {
        assert!(parse_java("bad.java", "\u{0}\u{1}\u{2}\u{3}not java at all{{{{").is_none());
    }

    #[test]
    fn parse_java_returns_some_on_valid_source() {
        let out = parse_java("Ok.java", "class Ok { void m() {} }\n");
        assert!(out.is_some());
    }

    #[test]
    fn parse_java_returns_none_on_comment_only_file_documented_deviation() {
        // Known parity deviation with TS/Python/Rust, mirrors zzop_parser_go's own F4 gap — see
        // `parse_tree`'s doc.
        assert!(parse_java("c.java", "// just a comment\n").is_none());
    }

    #[test]
    fn parse_java_returns_some_on_empty_file() {
        assert!(parse_java("empty.java", "").is_some());
    }

    #[test]
    fn count_loc_matches_workspace_convention() {
        assert_eq!(count_loc("a\nb\n"), 3);
        assert_eq!(count_loc(""), 1);
    }

    #[test]
    fn java_package_of_reads_dotted_package() {
        assert_eq!(
            java_package_of("package com.example.app;\nclass C {}\n"),
            Some("com.example.app".to_string())
        );
    }

    #[test]
    fn java_package_of_none_for_default_package() {
        assert_eq!(java_package_of("class C {}\n"), None);
    }

    #[test]
    fn java_type_names_lists_top_level_types_only() {
        let src = "class A { class Inner {} }\ninterface B {}\nrecord C(int x) {}\n";
        let names = java_type_names(src);
        assert_eq!(names, vec!["A", "B", "C"]);
    }
}
