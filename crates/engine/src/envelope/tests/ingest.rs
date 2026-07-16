//! Core Mode A ingestion tests — projection into the common IR, hand-built dep edges (imports,
//! re-exports, dynamic imports), degraded files, cycles, io surfacing, git gating, and `is_entry`.

use zzop_core::{ImportBinding, IoProvide, ReExport, SourceSymbol, SourceSymbolKind};

use crate::envelope::analyze_envelope;

use super::{config, envelope, projection};

#[test]
fn projects_loc_and_symbols_into_the_common_ir() {
    let mut a = projection("a.jsp", 10);
    a.symbols.push(SourceSymbol {
        id: "a.jsp#Foo".to_string(),
        file: "a.jsp".to_string(),
        name: "Foo".to_string(),
        kind: SourceSymbolKind::Class,
        line: 1,
        exported: true,
        is_default: false,
        body_start: None,
        body_end: None,
        write_sites: Vec::new(),
    });
    let env = envelope(vec![a]);
    let out = analyze_envelope(&env, &config());
    assert_eq!(out.file_count, 1);
    assert_eq!(out.ir.ir.loc.get("a.jsp"), Some(&10));
    assert_eq!(out.ir.ir.symbols.len(), 1);
    assert_eq!(out.ir.parser, "test-parser/1");
    assert_eq!(out.ir.source, "test");
}

#[test]
fn resolves_dep_edge_when_specifier_matches_a_projected_path() {
    let mut a = projection("a.jsp", 5);
    a.imports.insert(
        "b".to_string(),
        ImportBinding {
            specifier: "b.jsp".to_string(),
            original: "default".to_string(),
            deferred: false,
            type_only: false,
        },
    );
    let b = projection("b.jsp", 5);
    let env = envelope(vec![a, b]);
    let out = analyze_envelope(&env, &config());
    assert_eq!(
        out.ir.ir.dep.get("a.jsp").cloned().unwrap_or_default(),
        vec!["b.jsp".to_string()]
    );
    assert_eq!(
        out.ir.ir.dep.get("b.jsp").cloned().unwrap_or_default(),
        Vec::<String>::new()
    );
}

// --- Envelope-mode parity for Defect A: bare re-exports merge into the dep graph too ---

#[test]
fn bare_re_export_creates_dep_edge_and_gives_the_target_fan_in_in_envelope_mode() {
    // `export { x } from './impl'` with no local import of `impl` — mirrors
    // `zzop_parser_typescript::lang::resolve::build_dep_impl`'s own
    // `bare_named_re_export_creates_dep_edge`/`re_export_target_gains_fan_in_via_reverse_dep_edge`,
    // but through the envelope entry point (`analyze_envelope`), which builds `dep` by hand rather
    // than calling `build_dep_impl`.
    let mut barrel = projection("barrel.jsp", 5);
    barrel.re_exports.push(ReExport {
        specifier: "impl.jsp".to_string(),
        original: "x".to_string(),
        local_alias: "x".to_string(),
        type_only: false,
    });
    let impl_file = projection("impl.jsp", 5);
    let env = envelope(vec![barrel, impl_file]);
    let out = analyze_envelope(&env, &config());

    assert_eq!(
        out.ir.ir.dep.get("barrel.jsp").cloned().unwrap_or_default(),
        vec!["impl.jsp".to_string()]
    );
    // `impl.jsp` must not read as dead — some other file's `dep` entry now names it, i.e. it has
    // fan-in via the reverse edge, and `dead-candidates` (a whole-graph analysis run above) must not
    // flag it.
    let fan_in = out
        .ir
        .ir
        .dep
        .values()
        .filter(|tos| tos.contains(&"impl.jsp".to_string()))
        .count();
    assert_eq!(fan_in, 1);
    assert!(!out
        .findings
        .iter()
        .any(|f| f.rule_id == "dead-candidates" && f.file == "impl.jsp"));
}

#[test]
fn type_only_re_export_creates_excludable_dep_edge_in_envelope_mode() {
    // `export type { X } from './y'` is erased by TS at compile time, so it must never form a real
    // runtime cycle — but (Defect 1) it now DOES gain a real dep edge (fan-in), mirroring
    // `build_dep_impl`'s own `type_only_re_export_creates_excludable_dep_edge`.
    let mut barrel = projection("barrel.jsp", 5);
    barrel.re_exports.push(ReExport {
        specifier: "y.jsp".to_string(),
        original: "X".to_string(),
        local_alias: "X".to_string(),
        type_only: true,
    });
    let y_file = projection("y.jsp", 5);
    let env = envelope(vec![barrel, y_file]);
    let out = analyze_envelope(&env, &config());

    assert_eq!(
        out.ir.ir.dep.get("barrel.jsp").cloned().unwrap_or_default(),
        vec!["y.jsp".to_string()]
    );
    assert!(!out
        .findings
        .iter()
        .any(|f| f.rule_id == "dead-candidates" && f.file == "y.jsp"));
}

#[test]
fn dynamic_import_creates_excludable_dep_edge_in_envelope_mode() {
    // Defect 2 (envelope parity): a dynamic `import()` specifier used to create no dep edge at all,
    // so a code-split-only module looked dead. It now gains a real edge (fan-in), and the cycle it
    // would otherwise form with a mutual dynamic import is not reported (mirrors
    // `dynamic_import_creates_excludable_dep_edge`/`dynamic_import_cycle_is_not_reported_as_circular`
    // in `zzop_parser_typescript::lang::resolve`'s own tests).
    let mut page = projection("page.jsp", 5);
    page.dynamic_imports.push("chart.jsp".to_string());
    let chart = projection("chart.jsp", 5);
    let env = envelope(vec![page, chart]);
    let out = analyze_envelope(&env, &config());

    assert_eq!(
        out.ir.ir.dep.get("page.jsp").cloned().unwrap_or_default(),
        vec!["chart.jsp".to_string()]
    );
    assert!(!out
        .findings
        .iter()
        .any(|f| f.rule_id == "dead-candidates" && f.file == "chart.jsp"));
}

#[test]
fn unresolvable_specifier_is_external_not_an_error() {
    let mut a = projection("a.jsp", 5);
    a.imports.insert(
        "ext".to_string(),
        ImportBinding {
            specifier: "some/external/package".to_string(),
            original: "default".to_string(),
            deferred: false,
            type_only: false,
        },
    );
    let env = envelope(vec![a]);
    let out = analyze_envelope(&env, &config());
    assert!(out
        .ir
        .ir
        .dep
        .get("a.jsp")
        .cloned()
        .unwrap_or_default()
        .is_empty());
}

#[test]
fn degraded_file_is_reported_but_loc_still_counted() {
    let mut a = projection("a.jsp", 3);
    a.degraded = true;
    let env = envelope(vec![a]);
    let out = analyze_envelope(&env, &config());
    assert_eq!(out.degraded, vec!["a.jsp".to_string()]);
    assert_eq!(out.ir.ir.loc.get("a.jsp"), Some(&3));
}

#[test]
fn circular_import_pair_produces_a_circular_finding() {
    let mut a = projection("a.jsp", 5);
    a.imports.insert(
        "b".to_string(),
        ImportBinding {
            specifier: "b.jsp".to_string(),
            original: "default".to_string(),
            deferred: false,
            type_only: false,
        },
    );
    let mut b = projection("b.jsp", 5);
    b.imports.insert(
        "a".to_string(),
        ImportBinding {
            specifier: "a.jsp".to_string(),
            original: "default".to_string(),
            deferred: false,
            type_only: false,
        },
    );
    let env = envelope(vec![a, b]);
    let out = analyze_envelope(&env, &config());
    assert!(out.findings.iter().any(|f| f.rule_id == "circular"));
}

#[test]
fn mutual_dynamic_import_pair_does_not_produce_a_circular_finding_in_envelope_mode() {
    // Defect 2 (envelope parity): two files linked ONLY by dynamic `import()` (both directions) must
    // not read as a cycle — a value-import cycle between the same two files still must (covered by
    // `circular_import_pair_produces_a_circular_finding` above).
    let mut a = projection("a.jsp", 5);
    a.dynamic_imports.push("b.jsp".to_string());
    let mut b = projection("b.jsp", 5);
    b.dynamic_imports.push("a.jsp".to_string());
    let env = envelope(vec![a, b]);
    let out = analyze_envelope(&env, &config());
    assert!(!out.findings.iter().any(|f| f.rule_id == "circular"));
}

#[test]
fn io_facts_are_collected_and_surfaced_on_the_common_ir() {
    let mut a = projection("Ctrl.jsp", 20);
    a.io.provides.push(IoProvide {
        body: None,
        kind: "http".to_string(),
        key: "GET /legacy/user.jsp".to_string(),
        file: "Ctrl.jsp".to_string(),
        line: 3,
        symbol: None,
    });
    let env = envelope(vec![a]);
    let out = analyze_envelope(&env, &config());
    let io = out.ir.ir.io.expect("expected io facts");
    assert_eq!(io.provides.len(), 1);
    assert_eq!(io.provides[0].key, "GET /legacy/user.jsp");
}

#[test]
fn git_config_is_ignored_with_a_warning_and_never_panics() {
    let mut cfg = config();
    cfg.git = Some(crate::GitOptions::default());
    let env = envelope(vec![projection("a.jsp", 1)]);
    let out = analyze_envelope(&env, &cfg);
    assert!(out.scores.is_none());
    assert!(out.health.is_none());
    assert!(out
        .warnings
        .iter()
        .any(|w| w.contains("git collection skipped")));
}

#[test]
fn is_entry_projection_is_exempt_from_dead_candidates_in_envelope_mode() {
    // Mode A parity with the Mode B overlay union in `analyze::assemble`: an `is_entry`-marked
    // projection with zero fan-in (a crate root / test harness file, loaded by convention) must not
    // read as dead, while an unmarked zero-fan-in sibling still does.
    let mut entry = projection("lib.jsp", 5);
    entry.is_entry = true;
    let orphan = projection("orphan.jsp", 5);
    let out = analyze_envelope(&envelope(vec![entry, orphan]), &config());
    let dead: Vec<&str> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "dead-candidates")
        .map(|f| f.file.as_str())
        .collect();
    assert_eq!(dead, vec!["orphan.jsp"], "got findings: {:?}", out.findings);
}
