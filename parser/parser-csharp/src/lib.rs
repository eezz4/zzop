//! `zzop-parser-csharp` — a `tree-sitter-c-sharp`-based C# parser frontend -> Common IR projection,
//! mirroring `zzop-parser-go`'s tree-sitter discipline exactly and `zzop-parser-java-21`'s nested-type/
//! attribute-routing shape (grammar AST types stay inside this crate; only `zzop_core` types cross the
//! crate boundary — enforced by `scripts/check-tree-sitter-isolation.sh`'s allowlist, which the wiring
//! batch that adds this crate must extend).
//!
//! ## Layout
//! - `lang` — CST -> Common-IR LANGUAGE projection: `SourceSymbol` extraction (`symbols`, top-level +
//!   type-nested classes/interfaces/structs/records/delegates, methods/constructors/properties/consts
//!   with body spans), `ImportMap` extraction (`imports`, `using` directives), identifier-reference
//!   collection (`used_names`), and every namespace this file declares (`namespaces`).
//! - `adapters` — ASP.NET Core cross-layer IO: attribute-routed + minimal-API HTTP route PROVIDES
//!   (`adapters::provides`), and `HttpClient` literal HTTP egress CONSUMES (`adapters::http_clients`).
//!
//! ## Tree-sitter discipline (mirrors `zzop_parser_go`'s crate-root doc verbatim — see that crate for
//! the fuller rationale; summarized here)
//! - **Parse once per public fn call.** Every `pub fn` parses `text` exactly once via [`parse_tree`],
//!   then walks the resulting `tree_sitter::Tree`. Sibling public fns each parse independently.
//! - **Never-guess on parse errors.** [`parse_tree`] returns `None` when the root is hopeless (crate
//!   root gate below); a PARTIAL error elsewhere never blanks the rest of an otherwise-valid file —
//!   every walk in this crate skips just the erroring subtree via `util::valid_named_children`.
//! - **Node-kind vocabulary is pinned** — `node_kinds::PINNED_NODE_KINDS` (test-only), asserted against
//!   the compiled `tree_sitter_c_sharp::LANGUAGE`.
//! - **No tree-sitter types in the public API.**

pub mod adapters;
pub mod lang;
mod project;
mod util;

#[cfg(test)]
mod node_kinds;

pub use adapters::http_clients::extract_csharp_http_consumes;
pub use adapters::provides::extract_csharp_http_provides;
pub use lang::imports::parse_imports;
pub use lang::namespaces::csharp_namespaces_of;
pub use lang::symbols::parse_symbols;
pub use lang::used_names::parse_local_identifier_refs;
pub use project::{extract_csharp_http_provides_project, CSharpProjectProvidesReport};

/// Cache key ingredient for `zzop-cache`, mirroring `zzop_parser_go`/`zzop_parser_java_21::PARSER_FINGERPRINT`'s
/// scheme: parser id + pinned frontend + a logic-version counter.
/// - `v1`: initial release — symbols (top-level + nested class/struct/interface/record/delegate,
///   methods/constructors/properties, `const`/`static readonly` fields as `Const`), imports (plain/
///   static/alias/global `using`), `used_names`, `csharp_namespaces_of`, attribute-controller + minimal-
///   API HTTP provides, and `HttpClient` literal HTTP egress consumes.
/// - `+csharp-route-path-tristate-v1`: a NON-LITERAL route path (`[HttpGet(Routes.List)]`) or class-level
///   `[Route(ApiRoutes.Base)]` prefix — both valid C# `const`-string references — is no longer collapsed to
///   the empty base by the old `first_quoted_string(..).unwrap_or_default()`, which fabricated a phantom
///   route at the prefix (method path) / at the empty prefix (class path) AND lost the real one.
///   `attr_path_state` now distinguishes literal / absent-base / non-literal: a non-literal method path
///   drops the route, a non-literal class prefix blocks the class's own routes — the C# parallel of Java's
///   `+java-method-path-tristate-v1`.
/// - `+csharp-route-const-resolve-v1`: adds the whole-corpus `extract_csharp_http_provides_project` pass
///   (`crate::project`) that RESOLVES the non-literal route constants the per-file tri-state above drops —
///   a `[HttpGet(Routes.List)]` method path / `[Route(ApiRoutes.Base)]` class prefix whose `const string`
///   lives in another file. Bumped even though the PER-FILE `extract_csharp_http_provides` output is
///   byte-identical (the tri-state still drops/blocks with no corpus): the fingerprint tracks this crate's
///   route-extraction logic version, and the new resolution is part of it. The bump forces a one-time
///   re-projection of cached `.cs` per-file entries with unchanged results — harmless, and it keeps the
///   engine's uncached project pass (which reads every `.cs` fresh and REPLACES the per-file provides) the
///   single source of truth for C# `http` provides regardless of cache state.
pub const PARSER_FINGERPRINT: &str =
    "csharp/tree-sitter-c-sharp-0.23.5/v1+csharp-route-path-tristate-v1+csharp-route-const-resolve-v1";

/// Every top-level declaration kind this crate recognizes, PLUS `global_statement` (a top-level
/// executable statement — C#'s "top-level program" feature — never itself extracted, but still a sign
/// the file has SOME real C# in it) — the root-hopeless gate's "is there at least one of these among
/// the root's own top-level children?" set. Mirrors `zzop_parser_go::TOP_LEVEL_DECLARATION_KINDS`'s
/// exact role and doc. `namespace_declaration` covers the block form; `file_scoped_namespace_declaration`
/// the C# 10 `namespace X;` form.
const TOP_LEVEL_DECLARATION_KINDS: &[&str] = &[
    "using_directive",
    "namespace_declaration",
    "file_scoped_namespace_declaration",
    "class_declaration",
    "interface_declaration",
    "struct_declaration",
    "enum_declaration",
    "record_declaration",
    "delegate_declaration",
    "global_statement",
];

/// Parses `text` with `tree-sitter-c-sharp`, returning `None` when the root "fails to parse" — either
/// `Node::is_error()` on the root directly, or (the far more common real-world signal, mirroring
/// `zzop_parser_go`/`zzop_parser_java_21::parse_tree`'s identical two-gate shape) when NONE of the
/// root's own top-level children survive as a recognized, non-error/non-missing declaration kind
/// ([`TOP_LEVEL_DECLARATION_KINDS`]). A file with at least ONE valid top-level declaration alongside
/// broken ones still returns `Some` — a partial error elsewhere must not blank out an otherwise-fine
/// file.
///
/// Known parity deviation (deliberate, mirrors `zzop_parser_go`/`zzop_parser_java_21`'s own documented
/// F4 comment-only-file gap): a COMMENT-ONLY `.cs` file hits the second gate (its named children are
/// all `comment`, none a declaration) and is reported degraded, whereas TS/Python/Rust do not degrade a
/// comment-only file. Accepted for the same reason those two crates accept it: the only observable
/// difference is the `degraded` flag (such a file carries no symbols/imports either way), and an EMPTY
/// file (zero named children) short-circuits the `> 0` guard and is NOT degraded, matching every
/// sibling parser. Internal-only: `tree_sitter::Tree` never crosses this crate's public API.
pub(crate) fn parse_tree(text: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&csharp_language()).ok()?;
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

fn csharp_language() -> tree_sitter::Language {
    tree_sitter_c_sharp::LANGUAGE.into()
}

/// Raw physical line count — mirrors every other parser crate's `count_loc` exactly. The file is never
/// parsed here, so this is safe to call even when [`parse_tree`] would return `None`.
pub fn count_loc(text: &str) -> u32 {
    text.split('\n').count() as u32
}

/// Language projection: source -> `(symbols, imports, loc, used_names)`, the tuple mirroring
/// `zzop_parser_go::parse_go`/`zzop_parser_java_21::parse_java`'s pipeline slot shape. Returns `None`
/// when `parse_tree` fails on `text` — the caller degrades to a lexical fallback.
pub fn parse_csharp(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_csharp_returns_none_on_hopeless_input() {
        assert!(parse_csharp("bad.cs", "\u{0}\u{1}\u{2}\u{3}not csharp at all{{{{").is_none());
    }

    #[test]
    fn parse_csharp_returns_some_on_valid_source() {
        let out = parse_csharp("Ok.cs", "class Ok { void M() {} }\n");
        assert!(out.is_some());
    }

    #[test]
    fn parse_csharp_returns_none_on_comment_only_file_documented_deviation() {
        // Known parity deviation with TS/Python/Rust, mirrors zzop_parser_go/java's own F4 gap.
        assert!(parse_csharp("c.cs", "// just a comment\n").is_none());
    }

    #[test]
    fn parse_csharp_returns_some_on_empty_file() {
        assert!(parse_csharp("empty.cs", "").is_some());
    }

    #[test]
    fn count_loc_matches_workspace_convention() {
        assert_eq!(count_loc("a\nb\n"), 3);
        assert_eq!(count_loc(""), 1);
    }
}
