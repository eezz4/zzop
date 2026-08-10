//! Mode B `apply_adapter_overlays` — the APPLIED-vs-DECLARED contract of its returned
//! [`crate::envelope::OverlayApplication`]. The end-to-end effect of `entry_paths` on `dead-candidates`
//! is covered by `tests/analyze_adapter_overlay.rs`; these tests pin the source of truth itself, because
//! the defect they seal (a REJECTED overlay's `is_entry` still exempting its file) was invisible
//! end-to-end — it read as one fewer dead-candidate finding, not as an error.

use crate::envelope::apply_adapter_overlays;

use super::{envelope, projection};

/// A projection whose only declared fact is `is_entry: true` — the shape an adapter uses to say
/// "framework-loaded, not import-reached" (a SvelteKit `hooks.*`, a `.vue` route).
fn entry_projection(path: &str) -> zzop_core::FileProjection {
    let mut p = projection(path, 3);
    p.is_entry = true;
    p
}

#[test]
fn an_applied_overlays_is_entry_path_lands_in_entry_paths_and_covered_paths() {
    let overlays = vec![envelope(vec![
        entry_projection("src/hooks.server.ts"),
        projection("src/plain.svelte", 2),
    ])];
    let mut artifacts = Vec::new();
    let mut warnings = Vec::new();
    let applied = apply_adapter_overlays(&mut artifacts, &overlays, "test", &mut warnings);
    assert!(applied.entry_paths.contains("src/hooks.server.ts"));
    assert!(applied.covered_paths.contains("src/hooks.server.ts"));
    // A fact-less, non-entry projection is neither covered nor exempt (G8b).
    assert!(!applied.entry_paths.contains("src/plain.svelte"));
    assert!(!applied.covered_paths.contains("src/plain.svelte"));
}

/// THE SEAL: an overlay that fails `validate_envelope` contributes NOTHING — not even its `is_entry`
/// declaration. Reading `EngineConfig::adapter_overlays` directly (what `assemble::rules` used to do)
/// honored this rejected overlay's `is_entry`, leaving a dead file exempt from `dead-candidates` forever.
#[test]
fn a_rejected_overlay_contributes_no_entry_paths() {
    let mut bad = envelope(vec![entry_projection("src/hooks.server.ts")]);
    bad.format = "not-a-normalized-ast".to_string();
    let mut artifacts = Vec::new();
    let mut warnings = Vec::new();
    let applied = apply_adapter_overlays(&mut artifacts, &[bad], "test", &mut warnings);
    assert!(applied.entry_paths.is_empty());
    assert!(applied.covered_paths.is_empty());
    assert_eq!(
        warnings.len(),
        1,
        "the rejection is disclosed: {warnings:?}"
    );
    assert!(warnings[0].contains("skipped"));
}

/// A rejected overlay must not poison an ACCEPTED sibling in the same run — the skip is per-overlay.
#[test]
fn a_rejected_overlay_does_not_suppress_an_accepted_siblings_entry_path() {
    let mut bad = envelope(vec![entry_projection("src/rejected-entry.ts")]);
    bad.format = "not-a-normalized-ast".to_string();
    bad.parser = "bad-parser/1".to_string();
    let good = envelope(vec![entry_projection("src/good-entry.ts")]);
    let mut artifacts = Vec::new();
    let mut warnings = Vec::new();
    let applied = apply_adapter_overlays(&mut artifacts, &[bad, good], "test", &mut warnings);
    assert_eq!(
        applied
            .entry_paths
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["src/good-entry.ts"]
    );
}

fn binding(specifier: &str) -> zzop_core::ImportBinding {
    zzop_core::ImportBinding {
        specifier: specifier.to_string(),
        original: "default".to_string(),
        deferred: false,
        type_only: false,
    }
}

/// A projection carrying dep-graph facts on all three channels.
fn dep_graph_projection(
    path: &str,
    imports: &[(&str, &str)],
    re_export: Option<&str>,
    dynamic: Option<&str>,
) -> zzop_core::FileProjection {
    let mut p = projection(path, 10);
    for (local_name, specifier) in imports {
        p.imports
            .insert((*local_name).to_string(), binding(specifier));
    }
    if let Some(specifier) = re_export {
        p.re_exports.push(zzop_core::ReExport {
            specifier: specifier.to_string(),
            original: "X".to_string(),
            local_alias: "X".to_string(),
            type_only: false,
        });
    }
    if let Some(specifier) = dynamic {
        p.dynamic_imports.push(specifier.to_string());
    }
    p
}

/// THE SEAL for the dep-graph channels' all-or-nothing rule: a native artifact that bound even ONE
/// import used to discard the overlay's ENTIRE dep-graph contribution for that file, so an adapter's
/// worth was a function of what the native parser happened to leave empty rather than of what the
/// adapter knows (`examples/adapters/java-imports-adapter` became a total no-op the day the native Java
/// parser started emitting imports). Merging is now additive under a per-KEY native-first rule.
///
/// Run against the pre-change `merge_projection_onto_artifact` every assertion below except the first
/// fails — the first (native binding preserved) is what pins that additivity did not cost authority.
#[test]
fn an_overlay_adds_dep_graph_facts_a_native_artifact_lacks_without_overriding_the_ones_it_has() {
    // Stands in for a file the native pass really parsed: one bound local name, nothing else.
    let mut artifacts = vec![crate::envelope::merge::synthetic_artifact_from_projection(
        &dep_graph_projection("src/app.ts", &[("alreadyBound", "./native")], None, None),
    )];
    let overlays = vec![envelope(vec![dep_graph_projection(
        "src/app.ts",
        // `alreadyBound` collides with the native binding and must lose; `addedByAdapter` is new.
        &[
            ("alreadyBound", "./adapter-disagrees"),
            ("addedByAdapter", "./adapter"),
        ],
        Some("./re-exported"),
        Some("./dynamic"),
    )])];
    let mut warnings = Vec::new();
    apply_adapter_overlays(&mut artifacts, &overlays, "test", &mut warnings);

    let imports = artifacts[0].imports.as_ref().expect("dep-graph channel");
    assert_eq!(
        imports["alreadyBound"].specifier, "./native",
        "a native binding stays authoritative — the overlay never overrides a parsed fact"
    );
    assert_eq!(
        imports["addedByAdapter"].specifier, "./adapter",
        "a local name the native pass never bound is contributed by the overlay"
    );
    assert_eq!(artifacts[0].re_exports.len(), 1);
    assert_eq!(artifacts[0].dynamic_imports, vec!["./dynamic".to_string()]);
}

/// Additive merging must not duplicate: re-exports and dynamic specifiers have no key, so an overlay
/// restating a fact the native artifact already holds appends nothing. (`imports` cannot duplicate — the
/// map's own key handles it, pinned by the collision case above.)
#[test]
fn an_overlay_restating_a_native_dep_graph_fact_adds_no_duplicate() {
    let native = dep_graph_projection(
        "src/app.ts",
        &[("bound", "./native")],
        Some("./re-exported"),
        Some("./dynamic"),
    );
    let mut artifacts = vec![crate::envelope::merge::synthetic_artifact_from_projection(
        &native,
    )];
    let overlays = vec![envelope(vec![native.clone()])];
    let mut warnings = Vec::new();
    apply_adapter_overlays(&mut artifacts, &overlays, "test", &mut warnings);

    assert_eq!(artifacts[0].re_exports.len(), 1);
    assert_eq!(artifacts[0].dynamic_imports.len(), 1);
    assert_eq!(artifacts[0].imports.as_ref().unwrap().len(), 1);
}

/// A router-composition fragment channel is the one place additive merging can SUBTRACT. Fragments
/// compose into a single by-name graph (`analyze::compose_router_mount_provides` /
/// `compose_trpc_provides`): a fragment is a root only when no mount anywhere names it, and a mount by
/// import alias resolves to the target file's SOLE fragment. Two producers describing one file break
/// both — the file stops having a sole fragment, so an alias mount below it resolves to nothing and the
/// whole subtree goes silent. Measured on `examples/fastapi_overlay_adapter` against two FastAPI trees
/// the native Python parser already reads: provides 19 -> 0 and 25 -> 2.
///
/// The engine cannot pick a side (no `overrides` exists for this channel — it covers `imports` only) and
/// must not guess, so the composition's conservatism is right. What was missing is this line: without it
/// a route and a deleted route look identical in the output.
#[test]
fn an_overlay_describing_routers_another_producer_already_described_is_disclosed() {
    use zzop_core::{RouterMountEntry, RouterMountFragment};

    let mut native = projection("app/api.py", 10);
    native.router_mount_fragments.push(RouterMountFragment {
        name: "router".to_string(),
        entries: vec![RouterMountEntry::Verb {
            method: "GET".to_string(),
            path: "/me".to_string(),
            handler: Some("current_user".to_string()),
            line: 4,
            attr_keys: vec![],
        }],
    });
    let mut artifacts = vec![crate::envelope::merge::synthetic_artifact_from_projection(
        &native,
    )];
    let overlays = vec![envelope(vec![native.clone()])];
    let mut warnings = Vec::new();
    apply_adapter_overlays(&mut artifacts, &overlays, "test", &mut warnings);

    let line = warnings
        .iter()
        .find(|w| w.contains("router-mount"))
        .unwrap_or_else(|| panic!("expected a fragment-collision disclosure, got {warnings:?}"));
    assert!(line.contains("app/api.py"), "{line}");
    assert!(line.contains("router"), "{line}");
    assert!(
        line.contains("test-parser/1"),
        "the disclosure names the producer that collided: {line}"
    );
}

/// The mirror: the SUPPORTED shape must stay silent. An overlay that describes routers for a file no
/// other producer described is exactly what Mode B exists for — including the cross-producer case where
/// a natively-parsed parent mounts an overlay-supplied child, which per-producer scoping would have
/// forbidden. Only a file described TWICE in one channel is a collision.
#[test]
fn an_overlay_supplying_routers_no_one_else_described_is_not_disclosed() {
    use zzop_core::{RouterMountEntry, RouterMountFragment};

    let native = projection("app/api.py", 10);
    let mut overlay_only = projection("app/legacy.py", 8);
    overlay_only
        .router_mount_fragments
        .push(RouterMountFragment {
            name: "router".to_string(),
            entries: vec![RouterMountEntry::Verb {
                method: "GET".to_string(),
                path: "/me".to_string(),
                handler: Some("current_user".to_string()),
                line: 4,
                attr_keys: vec![],
            }],
        });
    let mut artifacts = vec![crate::envelope::merge::synthetic_artifact_from_projection(
        &native,
    )];
    let overlays = vec![envelope(vec![overlay_only])];
    let mut warnings = Vec::new();
    apply_adapter_overlays(&mut artifacts, &overlays, "test", &mut warnings);

    assert!(
        !warnings.iter().any(|w| w.contains("router-mount")),
        "{warnings:?}"
    );
}
