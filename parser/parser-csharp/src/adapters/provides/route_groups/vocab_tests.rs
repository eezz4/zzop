//! What `vocabulary.csharpRootRouteBuilderVariableNames` actually decides — the four cases that make
//! this a DECLARED name rather than a built-in guess, each written so it goes red the moment the
//! declaration stops being read.
//!
//! The seam under test is `super::head_prefix`, exercised through the real entry point
//! (`crate::extract_csharp_http_provides`) rather than directly, so a case here fails if ANY link in the
//! chain drops the vocabulary on the floor.

use super::CSharpRouteVocab;

/// One file's minimal-API route keys, sorted, under an explicitly declared root vocabulary.
fn keys_under(root_names: &[&str], src: &str) -> Vec<String> {
    let vocab = CSharpRouteVocab {
        root_route_builder_variable_names: root_names,
    };
    let mut keys: Vec<String> = crate::extract_csharp_http_provides("Program.cs", src, &vocab)
        .into_iter()
        .map(|p| p.key)
        .collect();
    keys.sort();
    keys
}

/// Two registrations on two different root spellings — `app` (zzop's own suggested value) and
/// `builder` (527 of aspnetcore's 1990 bare receivers). Every case below runs on this same source, so
/// the only thing that moves between them is the declaration.
const TWO_ROOTS: &str = "var app = builder.Build();\napp.MapGet(\"/from-app\", () => \"a\");\nbuilder.MapGet(\"/from-builder\", () => \"b\");\n";

#[test]
fn the_shipped_default_keys_app_and_leaves_every_other_root_name_to_be_declared() {
    assert_eq!(
        keys_under(super::DEFAULT_ROOT_ROUTE_BUILDER_VARIABLE_NAMES, TWO_ROOTS),
        vec!["GET /from-app"],
        "the shipped default is the single name `app` — see its own doc for what that reaches"
    );
}

/// The `vocabulary` roof's whole-replacement contract, on this key: a declared list REPLACES the
/// built-in outright, so declaring `builder` does not ALSO keep `app`. An element-wise merge would
/// return both keys here, which is exactly the "two authors, neither can say what the effective set is"
/// failure `crates/config/config-surface.json` refuses.
#[test]
fn a_declared_name_replaces_the_default_rather_than_adding_to_it() {
    assert_eq!(
        keys_under(&["builder"], TWO_ROOTS),
        vec!["GET /from-builder"],
        "declaring `builder` must key `builder` AND stop keying `app`"
    );
}

/// "Absent or empty means the judgment is NOT MADE" reaching this adapter: with nothing declared no
/// bare receiver is the root, so both registrations are SKIPPED rather than keyed at a guessed path.
/// This is the case that fails if anyone ever puts a built-in fallback back into `head_prefix`.
#[test]
fn an_empty_declaration_recognises_no_root_at_all() {
    assert!(
        keys_under(&[], TWO_ROOTS).is_empty(),
        "an undeclared vocabulary must under-extract, never fall back to a built-in guess"
    );
}

/// The key's own scope boundary: a route GROUP is resolved structurally and is NOT this vocabulary's
/// business. `api` is never declared here, yet its routes key under the group's prefix — and the
/// declared root still contributes the empty prefix at the head of that chain.
#[test]
fn a_group_variable_keeps_its_prefix_without_being_declared() {
    let src = "var app = builder.Build();\nvar api = app.MapGroup(\"/api\");\napi.MapGet(\"/items\", () => \"x\");\n";
    assert_eq!(keys_under(&["app"], src), vec!["GET /api/items"]);
    // ...and declaring the GROUP name instead of the root is the misuse the field doc warns about: it
    // keys the same registration at the root, losing `/api`. Pinned so the cost is visible, not implied.
    assert_eq!(keys_under(&["api"], src), vec!["GET /items"]);
}
