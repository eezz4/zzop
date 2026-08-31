//! Unit pins for the shared noncycle fold — one per guard, each asserting the guard's own direction.
//! The engine-level counterparts (whole trees through `analyze_tree`) live in
//! `crates/engine/tests/integration/analyze_type_only_export_cycle.rs`; these pin the decision itself,
//! including the cases no TypeScript tree can express (a Java target riding the same builder).

use super::NoncycleCandidates;
use crate::ir::{SourceSymbol, SourceSymbolKind};

fn symbol(file: &str, name: &str, kind: SourceSymbolKind, exported: bool) -> SourceSymbol {
    SourceSymbol {
        id: format!("{file}#{name}"),
        file: file.to_string(),
        name: name.to_string(),
        kind,
        line: 1,
        exported,
        is_default: false,
        body_start: None,
        body_end: None,
        write_sites: Vec::new(),
    }
}

fn one(from: &str, to: &str, name: &str, erased: bool) -> NoncycleCandidates {
    let mut c = NoncycleCandidates::new();
    c.record(from, to, name, erased);
    c
}

#[test]
fn an_exported_interface_declaration_erases_a_value_spelled_import() {
    let c = one("svc.ts", "ctrl.ts", "CalendarState", false);
    let symbols = [symbol(
        "ctrl.ts",
        "CalendarState",
        SourceSymbolKind::Interface,
        true,
    )];
    assert!(c
        .refine(&symbols, true)
        .contains(&("svc.ts".into(), "ctrl.ts".into())));
}

#[test]
fn an_exported_value_declaration_does_not() {
    let c = one("svc.ts", "ctrl.ts", "makeState", false);
    let symbols = [symbol(
        "ctrl.ts",
        "makeState",
        SourceSymbolKind::Const,
        true,
    )];
    assert!(c.refine(&symbols, true).is_empty());
}

/// Guard (b), the one that keeps a real cycle alive: a TypeScript `enum` projects NO symbol
/// (`Decl::TsEnum` is dropped by the front end) and neither does a degraded file. Silence must read as
/// "keep the edge", never as "it is a type".
#[test]
fn a_name_with_no_declaration_at_all_is_never_erased() {
    let c = one("ctrl.ts", "locales.ts", "Locales", false);
    // The target file projects OTHER symbols — this is a real, non-degraded parse whose `enum` simply
    // never reaches the IR, which is exactly the shape that must not be read as evidence.
    let symbols = [symbol(
        "locales.ts",
        "helper",
        SourceSymbolKind::Function,
        true,
    )];
    assert!(c.refine(&symbols, true).is_empty());
}

/// Guard (d), declaration merging: `interface Merged` and `const Merged` legally coexist and collapse
/// onto ONE `SourceSymbol::id`, so the lookup keys on `(file, name)` and demands every sibling be a
/// type. The value sibling keeps the edge.
#[test]
fn a_declaration_merged_value_sibling_keeps_the_edge() {
    let c = one("ctrl.ts", "model.ts", "Merged", false);
    let symbols = [
        symbol("model.ts", "Merged", SourceSymbolKind::Interface, true),
        symbol("model.ts", "Merged", SourceSymbolKind::Const, true),
    ];
    assert!(c.refine(&symbols, true).is_empty());
    // Control in the same assertion's reach: drop the value sibling and the very same edge IS erased,
    // so the emptiness above is the merge doing the work, not the fixture failing to match.
    let type_only_side = [symbol(
        "model.ts",
        "Merged",
        SourceSymbolKind::Interface,
        true,
    )];
    assert!(!c.refine(&type_only_side, true).is_empty());
}

/// Guard (a): `SourceSymbolKind::Interface` also means a Java `interface`, a Rust `trait` and a Go
/// `interface`, none of which is erased by anything — and non-TypeScript `ImportMap`s ride the same
/// dep-graph builder. Only a TypeScript-family target may take the export-side arm.
#[test]
fn a_non_typescript_target_never_takes_the_export_side_arm() {
    let java = one("Ctrl.java", "Repo.java", "Repo", false);
    let symbols = [symbol(
        "Repo.java",
        "Repo",
        SourceSymbolKind::Interface,
        true,
    )];
    assert!(
        java.refine(&symbols, true).is_empty(),
        "a Java interface is a runtime type — the edge must survive"
    );
    // Control: the identical shape with a `.ts` target IS erased, proving the extension is what
    // decided it rather than some other mismatch in the fixture.
    let ts = one("ctrl.ts", "repo.ts", "Repo", false);
    let ts_symbols = [symbol("repo.ts", "Repo", SourceSymbolKind::Interface, true)];
    assert!(!ts.refine(&ts_symbols, true).is_empty());
}

/// Guard (c): with the tree-level gate off (`verbatimModuleSyntax` and friends) the fold reduces
/// exactly to the import-side-only behavior that predates the export-side arm.
#[test]
fn the_tree_level_gate_off_reduces_to_import_side_only() {
    let value_spelled = one("svc.ts", "ctrl.ts", "CalendarState", false);
    let symbols = [symbol(
        "ctrl.ts",
        "CalendarState",
        SourceSymbolKind::Interface,
        true,
    )];
    assert!(value_spelled.refine(&symbols, false).is_empty());
    // `import type` still excludes with the gate off — it never needed export-side evidence.
    let type_spelled = one("svc.ts", "ctrl.ts", "CalendarState", true);
    assert!(!type_spelled.refine(&symbols, false).is_empty());
}

#[test]
fn one_value_binding_among_erasable_ones_keeps_the_whole_pair() {
    let mut c = NoncycleCandidates::new();
    c.record("b.ts", "i.ts", "EventTypeWithOwner", false);
    c.record("b.ts", "i.ts", "eventTypeBookingFieldsSchema", false);
    let symbols = [
        symbol("i.ts", "EventTypeWithOwner", SourceSymbolKind::Type, true),
        symbol(
            "i.ts",
            "eventTypeBookingFieldsSchema",
            SourceSymbolKind::Const,
            true,
        ),
    ];
    assert!(c.refine(&symbols, true).is_empty());
}

/// The synthetic `original` spellings need no special case — nothing is DECLARED under any of them,
/// so the export-side arm finds nothing and each edge survives.
#[test]
fn namespace_default_and_side_effect_originals_are_never_erased() {
    let symbols = [
        symbol("y.ts", "Thing", SourceSymbolKind::Interface, true),
        symbol("y.ts", "other", SourceSymbolKind::Type, true),
    ];
    for original in ["*", "default", "_"] {
        let c = one("x.ts", "y.ts", original, false);
        assert!(
            c.refine(&symbols, true).is_empty(),
            "`{original}` names no declaration — the edge must survive"
        );
    }
}

/// A declaration that is not exported is not evidence about what an import of that name resolves to.
#[test]
fn an_unexported_type_declaration_is_not_evidence() {
    let c = one("svc.ts", "ctrl.ts", "Local", false);
    let symbols = [symbol("ctrl.ts", "Local", SourceSymbolKind::Type, false)];
    assert!(c.refine(&symbols, true).is_empty());
}
