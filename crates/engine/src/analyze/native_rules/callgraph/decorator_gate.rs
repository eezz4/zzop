//! Helpers for `run_callgraph_rules`'s decorator-guard evidence gate — split out of `mod.rs` purely to
//! stay under the repo's per-file line cap; every item here is `pub(super)`, used only by `callgraph::mod`.

use zzop_core::{is_enabled, Matcher};

use crate::EngineConfig;

/// Whether at least one loaded+enabled DSL pack has an `IoScan` rule that would actually READ the
/// decorator-guard evidence `run_callgraph_rules` produces (`attr_present`/`attr_absent`, the vocab-free
/// `AttributeStore` gate `assemble/rules/io_scan.rs`'s `mint_auth_guarded` feeds — e.g. the shipped http
/// pack's `auth-gates`, post-A2-migration). Gated identically to `io_scan::run`'s own pack/rule enablement
/// (`is_enabled` at the pack level, then `pipeline::gate_pack_rules` per rule) so this predicate can never
/// disagree with what actually runs. This is the OTHER consumer `run_callgraph_rules`'s
/// `need_decorator_guarded` ORs against `run_mutating_no_auth` — see that binding's doc for why.
pub(super) fn packs_read_io_scan_attrs(config: &EngineConfig) -> bool {
    config
        .packs
        .iter()
        .filter(|p| is_enabled(&config.rule_config, &p.id))
        .map(|p| crate::pipeline::gate_pack_rules(p, &config.rule_config))
        .any(|gated| {
            gated.rules.iter().any(|r| {
                matches!(&r.matcher, Matcher::IoScan(m) if m.attr_present.is_some() || m.attr_absent.is_some())
            })
        })
}

/// Whether a route provide's path (leading-slash, `http_interface_key`-normalized, and already carrying
/// the app's NestJS global prefix if one exists — `/api/articles/{}/comments`) is EXACTLY the route a
/// NestJS `forRoutes` PATTERN covers (controller-relative, no leading slash, no global prefix —
/// `articles/{}/comments`). The pattern is reconciled to the provide's key space by prepending
/// `global_prefix` (when a literal one was found) and comparing for EQUALITY — not a suffix match, which
/// would over-clear (a `{path:'articles'}` pattern must not exempt an unrelated `/api/admin/articles`
/// route in another module). Both sides already share the `{}` param normalization. When `global_prefix`
/// is `None` (no `setGlobalPrefix`, or a non-literal one that can't be read), the pattern is matched
/// unprefixed; if the app truly has a prefix we failed to read, the exemption is simply MISSED (the
/// finding stays) — never an over-clear, the safe direction for a security rule.
pub(super) fn forroutes_path_matches(
    provide_path: &str,
    pattern: &str,
    global_prefix: Option<&str>,
) -> bool {
    let pat = pattern.trim_start_matches('/');
    let expected = match global_prefix {
        Some(p) if !p.trim_matches('/').is_empty() => format!("/{}/{}", p.trim_matches('/'), pat),
        _ => format!("/{pat}"),
    };
    provide_path == expected
}

/// The Java source-root prefix a Spring security config governs — everything up to and including the first
/// `src/main/java/` segment (the Maven/Gradle convention), so a posture only exempts routes in its OWN
/// module. A monorepo module lives at `<module>/src/main/java/...`, so `service-a`'s config yields prefix
/// `service-a/src/main/java/` and can never match `service-b/src/main/java/...`. When the config isn't
/// under a recognizable source root (unusual layout), falls back to the config file's own directory — the
/// most conservative scope (only same-directory routes), never the whole tree.
pub(super) fn spring_app_root(config_file: &str) -> &str {
    const SRC_ROOT: &str = "src/main/java/";
    if let Some(idx) = config_file.find(SRC_ROOT) {
        &config_file[..idx + SRC_ROOT.len()]
    } else {
        match config_file.rfind('/') {
            Some(i) => &config_file[..=i],
            None => "",
        }
    }
}
