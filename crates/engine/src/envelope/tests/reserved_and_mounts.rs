//! Reserved engine-internal sentinel-kind drops (Mode A parity with the Mode B overlay filter) and
//! `EngineConfig::mounts` parity in envelope mode.

use zzop_core::{IoConsume, IoProvide};

use crate::envelope::analyze_envelope;

use super::{config, envelope, projection};

// --- Reserved engine-internal sentinel kinds are producer-forbidden in envelopes too (Mode A parity
// with the Mode B overlay filter — `apply_adapter_overlays`'s own tests in
// `tests/analyze_adapter_overlay.rs` cover the identical contract for overlays) ---

#[test]
fn nest_global_prefix_provide_is_dropped_and_warned_in_envelope_mode() {
    let mut a = projection("legacy.jsp", 3);
    a.io.provides.push(IoProvide {
        body: None,
        kind: zzop_parser_typescript::NEST_GLOBAL_PREFIX_KIND.to_string(),
        key: "api".to_string(),
        file: "legacy.jsp".to_string(),
        line: 1,
        symbol: None,
    });
    // An ordinary sibling route, so `io` is `Some` and we can also pin it comes through untouched —
    // envelope mode never runs `apply_and_strip_global_prefix` at all (module doc), so there is no
    // tree-wide re-prefix step to prove absent here the way the Mode B e2e test does; this instead
    // pins that the drop itself does not disturb an ordinary sibling provide.
    a.io.provides.push(IoProvide {
        body: None,
        kind: "http".to_string(),
        key: "GET /widgets".to_string(),
        file: "legacy.jsp".to_string(),
        line: 2,
        symbol: None,
    });
    let env = envelope(vec![a]);
    let out = analyze_envelope(&env, &config());

    let provides = out.ir.ir.io.expect("expected io facts").provides;
    assert!(
        !provides
            .iter()
            .any(|p| p.kind == zzop_parser_typescript::NEST_GLOBAL_PREFIX_KIND),
        "{:?}",
        provides
    );
    assert!(
        provides
            .iter()
            .any(|p| p.kind == "http" && p.key == "GET /widgets"),
        "{:?}",
        provides
    );
    assert!(
        out.warnings.iter().any(|w| w.contains("test-parser/1")
            && w.contains("dropped 1 reserved engine-internal io entry")
            && w.contains("nest-global-prefix")),
        "{:?}",
        out.warnings
    );
}

#[test]
fn client_base_prefix_consume_is_dropped_and_warned_in_envelope_mode() {
    // `IoConsume`-side counterpart of the provide test above: an envelope emitting
    // `zzop_parser_typescript::CLIENT_BASE_PREFIX_KIND` directly is producer-forbidden the same way.
    let mut a = projection("legacy.jsp", 2);
    a.io.consumes.push(IoConsume {
        kind: zzop_parser_typescript::CLIENT_BASE_PREFIX_KIND.to_string(),
        key: Some("/api".to_string()),
        file: "legacy.jsp".to_string(),
        line: 1,
        raw: None,
        method: None,
        retry_configured: None,
        body: None,
        client: Some("axios".to_string()),
    });
    let env = envelope(vec![a]);
    let out = analyze_envelope(&env, &config());

    let consumes = out.ir.ir.io.map(|io| io.consumes).unwrap_or_default();
    assert!(
        !consumes
            .iter()
            .any(|c| c.kind == zzop_parser_typescript::CLIENT_BASE_PREFIX_KIND),
        "{:?}",
        consumes
    );
    assert!(
        out.warnings.iter().any(|w| w.contains("test-parser/1")
            && w.contains("dropped 1 reserved engine-internal io entry")
            && w.contains("client-base-prefix")),
        "{:?}",
        out.warnings
    );
}

#[test]
fn ordinary_io_kinds_are_not_dropped_or_warned_in_envelope_mode() {
    // Control case: an envelope whose `io` carries only ordinary (non-reserved) kinds must pass
    // through in full, with no drop warning at all — the reserved-kind filter must not have
    // false-positive reach (mirrors `overlay_with_only_ordinary_io_kinds_merges_with_no_drop_warning`).
    let mut a = projection("a.jsp", 3);
    a.io.provides.push(IoProvide {
        body: None,
        kind: "http".to_string(),
        key: "GET /widgets".to_string(),
        file: "a.jsp".to_string(),
        line: 2,
        symbol: None,
    });
    a.io.consumes.push(IoConsume {
        kind: "http".to_string(),
        key: Some("/widgets".to_string()),
        file: "a.jsp".to_string(),
        line: 3,
        raw: None,
        method: Some("GET".to_string()),
        retry_configured: None,
        body: None,
        client: None,
    });
    let env = envelope(vec![a]);
    let out = analyze_envelope(&env, &config());

    let io = out.ir.ir.io.expect("expected io facts");
    assert_eq!(io.provides.len(), 1);
    assert_eq!(io.consumes.len(), 1);
    assert!(
        !out.warnings.iter().any(|w| w.contains("dropped")),
        "{:?}",
        out.warnings
    );
}

// --- `EngineConfig::mounts` parity: Mode A must apply config mounts too (audited consistency gap —
// `apply_config_mounts` used to run only in the native `analyze::assemble` path, so a tree analyzed
// via `analyze_envelope` with the same `mounts` config silently froze un-mounted keys). ---

#[test]
fn config_mount_prepends_gateway_prefix_to_an_http_provide_key_in_envelope_mode() {
    let mut a = projection("users.jsp", 5);
    a.io.provides.push(IoProvide {
        body: None,
        kind: "http".to_string(),
        key: "GET /users".to_string(),
        file: "users.jsp".to_string(),
        line: 1,
        symbol: None,
    });
    let env = envelope(vec![a]);
    let mut cfg = config();
    cfg.mounts = vec![crate::MountRule {
        dir: String::new(),
        at: "/gw".to_string(),
    }];
    let out = analyze_envelope(&env, &cfg);
    let provides = out.ir.ir.io.expect("expected io facts").provides;
    assert!(
        provides
            .iter()
            .any(|p| p.kind == "http" && p.key == "GET /gw/users"),
        "{:?}",
        provides
    );
}

#[test]
fn config_mount_matching_nothing_emits_the_same_had_no_effect_warning_as_the_native_path_in_envelope_mode(
) {
    let mut a = projection("users.jsp", 5);
    a.io.provides.push(IoProvide {
        body: None,
        kind: "http".to_string(),
        key: "GET /users".to_string(),
        file: "users.jsp".to_string(),
        line: 1,
        symbol: None,
    });
    let env = envelope(vec![a]);
    let mut cfg = config();
    cfg.mounts = vec![crate::MountRule {
        dir: "nowhere".to_string(),
        at: "/gw".to_string(),
    }];
    let out = analyze_envelope(&env, &cfg);
    assert!(
        out.warnings.iter().any(|w| w.contains(
            "topology mount \"gw\" (dir \"nowhere\") had no effect: 0 http provides matched"
        )),
        "{:?}",
        out.warnings
    );
}

#[test]
fn config_mount_leaves_non_http_provide_kinds_untouched_in_envelope_mode() {
    let mut a = projection("router.jsp", 5);
    a.io.provides.push(IoProvide {
        body: None,
        kind: "trpc".to_string(),
        key: "widgets.list".to_string(),
        file: "router.jsp".to_string(),
        line: 1,
        symbol: None,
    });
    let env = envelope(vec![a]);
    let mut cfg = config();
    cfg.mounts = vec![crate::MountRule {
        dir: String::new(),
        at: "/gw".to_string(),
    }];
    let out = analyze_envelope(&env, &cfg);
    let provides = out.ir.ir.io.expect("expected io facts").provides;
    assert!(
        provides
            .iter()
            .any(|p| p.kind == "trpc" && p.key == "widgets.list"),
        "{:?}",
        provides
    );
}
