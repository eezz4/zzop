//! S18: protected-path auth RANGE self-report — names, per run, the mainstream auth idioms this engine
//! cannot witness per route, on the runs where they can actually cost a false positive.
//!
//! ## What this exists for
//! `http/protected-path-no-auth-evidence` clears a route on ONE thing: an `auth-guarded` attribute,
//! minted from the per-route evidence `run_callgraph_rules` recognizes (`analyze::assemble::rules::
//! io_scan`'s `mint_auth_guarded`, fed by `native_rules::callgraph::decorator_gate`). Applying auth
//! ANYWHERE but the route's own registration site produces no such evidence, and three ways of doing
//! that are mainstream rather than marginal — one measured in each of the three corpus trees that fire
//! this rule at all:
//!
//! 1. **Registered globally rather than per route** — NestJS's `APP_GUARD` provider or
//!    `app.useGlobalGuards(..)`, a servlet filter chain. The controller carries no guard decorator
//!    because the guard is not attached there.
//! 2. **Folded into a COMPOSITE decorator the project defines itself** — `applyDecorators(..)`. The
//!    route reads `@Authenticated({..})`; the guard evidence, if any, is inside a factory in another
//!    file. `adapters::controller_decorators`' module doc already lists `applyDecorators` among the
//!    shapes it does not detect, and its "Known residual" paragraph already says a controller relying
//!    on a global guard "still false-positives on the consuming rule". This tripwire is that sentence
//!    moved from a source comment, which no user reads, to the run output, which they do.
//! 3. **Applied to the ROUTER, not the route** — a tRPC `authedAdminProcedure` behind a Next.js API
//!    file that only mounts the router, an Express router-level `.use(..)` outside `router_mounts`'
//!    resolved shapes. The registration site names a handler and nothing else.
//!
//! Measured on a 42-route NestJS tree carrying both: 36 of 42 findings were routes guarded by a
//! project-defined composite decorator enforced by an `APP_GUARD`, and the tree contained no
//! `@UseGuards` anywhere — so the recognizer had nothing to recognize and the rule reported the whole
//! controller surface.
//!
//! ## Why disclosed rather than modeled — and why the finding count is NOT reduced
//! On that same tree, 6 of the 42 are TRUE: three routes carry the composite decorator with its own
//! `{ public: true }` option and three carry no auth decorator at all while every sibling in their file
//! does. The name of the decorator is identical across the true and the false ones; only its ARGUMENT
//! separates them, and reading a route decorator's options to decide auth is a concept
//! `adapters::controller_decorators` already judged out of scope ("a `provide` carries its own
//! auth-exemption", the `{ skipAuth: true }` note). So there is no per-route rewrite that clears the
//! false ones without also clearing the true ones, and a rule change that reached zero here would
//! silence the exact routes the rule exists to surface. Disclosure costs no recall, which is why this
//! is a `warnings` entry and not a matcher edit.
//!
//! ## Gate: the rule's own range
//! Fires only when this tree registers at least one http route under a protected path segment in a
//! language the rule scans — exactly the population it evaluates — and only when the rule is actually
//! enabled for this run (naming a rule the user switched off is a worse answer than naming none, the
//! same judgment `channel_consequence` makes). A tree with no protected-path route, or one whose
//! routes are all in Go/C#, stays silent. Like S10 it does NOT try to detect whether this tree really
//! uses a global or composite guard: that would be a lexical guess, and the honest claim is about
//! RANGE, which holds either way.
//!
//! That gate is deliberately wider than "the rule fired here", and one corpus tree shows the cost and
//! the reason together: it registers two `/internal/` routes that carry a LITERAL `@UseGuards(..)`, so
//! the recognizer sees them, the rule clears both, and this disclosure still fires on a tree where the
//! rule was precise. Two things make that the right trade. It is the positive control for the whole
//! producer — the same corpus round where a 42-route tree got no per-route evidence at all has a tree
//! where the evidence path works end to end, so a reader is not being told the recognizer is broken.
//! And narrowing to "fired at least once" would need the finding set threaded into this phase, which
//! would make a disclosure about RANGE depend on an OUTCOME: the run where every protected route is
//! wrongly cleared by an over-broad overlay is exactly the run that would then say nothing.

use zzop_core::{is_enabled, is_pack_enabled, IoProvide};

/// The pack whose enablement carries [`AFFECTED_RULE_ID`], and the rule id itself. Spelled here rather
/// than inline for the reason S8/S9/S10 and `orm_schema_silence` each spell theirs: a DSL pack exposes
/// its id only as JSON text, so the only thing between this disclosure and a ghost id is a pin —
/// `the_affected_rule_id_is_a_real_shipped_dsl_rule` below reads the shipped pack and is that pin.
const AFFECTED_PACK_ID: &str = "http";
const AFFECTED_RULE_ID: &str = "http/protected-path-no-auth-evidence";

/// The file extensions [`AFFECTED_RULE_ID`] scans, mirroring its `file_pattern`. This is a THIRD place
/// that has to agree with the rule's language scope (the other two are the pack's own `file_pattern`
/// and `decorator_gate`'s producer set), which is a debt rather than a design — so it is the one of the
/// three that cannot drift silently: `the_scope_matches_the_shipped_rule` parses the shipped pack and
/// fails the moment the two disagree in either direction. Go and C# are absent on purpose; they have no
/// `auth-guarded` producer, the rule does not scan them, and disclosing a rule's blind spot on routes it
/// never evaluates is noise.
const SCOPED_EXTENSIONS: &[&str] = &[
    "ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "java", "py",
];

/// The path segments [`AFFECTED_RULE_ID`] treats as protected, mirroring its `key_pattern`. Pinned by
/// `the_protected_segments_match_the_shipped_rule` for the same reason as the scope above.
const PROTECTED_SEGMENTS: &[&str] = &["admin", "internal"];

/// Cap on example route files listed — the "up to 3 example paths" convention every sibling uses.
const MAX_EXAMPLES: usize = 3;

/// Whether `file`'s extension is one the affected rule scans.
fn in_scope(file: &str) -> bool {
    let ext = file.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    ext.is_some_and(|e| SCOPED_EXTENSIONS.contains(&e.as_str()))
}

/// Whether a route path carries a protected segment, matched the way the rule's `key_pattern` matches
/// it — a whole path segment, so `/admin/x` and a trailing `/admin` count while `/administrators` does
/// not.
fn is_protected_path(path: &str) -> bool {
    path.split('/').any(|seg| {
        PROTECTED_SEGMENTS
            .iter()
            .any(|p| seg.eq_ignore_ascii_case(p))
    })
}

/// `Some(warning)` when this tree registers at least one protected-path http route in a language the
/// affected rule scans AND that rule is enabled for this run. `None` otherwise.
pub fn protected_path_auth_range_warning(
    io_provides: &[IoProvide],
    gate: &zzop_core::RuleConfig,
) -> Option<String> {
    if !is_pack_enabled(gate, AFFECTED_PACK_ID) || !is_enabled(gate, AFFECTED_RULE_ID) {
        return None;
    }
    let mut count = 0usize;
    let mut examples: Vec<String> = Vec::new();
    for p in io_provides
        .iter()
        .filter(|p| p.kind == "http" && in_scope(&p.file))
    {
        let path = p
            .key
            .split_once(' ')
            .map_or(p.key.as_str(), |(_, rest)| rest);
        if !is_protected_path(path) {
            continue;
        }
        count += 1;
        if examples.len() < MAX_EXAMPLES && !examples.contains(&p.file) {
            examples.push(p.file.clone());
        }
    }
    if count == 0 {
        return None;
    }
    Some(format!(
        "Protected-path auth-range gap: {count} http route(s) are registered under a protected path \
         segment (e.g. {}), so `{AFFECTED_RULE_ID}` evaluates them. It clears a route only on auth \
         evidence it can witness ON THAT ROUTE: Express middleware, NestJS `@UseGuards`/`forRoutes`, a \
         Spring method-security annotation, FastAPI `Depends`, DRF `permission_classes`, or an injected \
         `auth-guarded` attribute. A guard applied ANYWHERE ELSE than the route's own registration site \
         is outside that set, and a route guarded only that way WILL be reported. Three such idioms are \
         mainstream rather than marginal: (1) a guard registered GLOBALLY instead of per route (a NestJS \
         `APP_GUARD` provider or `app.useGlobalGuards(..)`, a servlet filter chain), which leaves no \
         decorator on the controller to read; (2) a guard folded into a COMPOSITE decorator the project \
         defines itself (`applyDecorators(..)`), whose expansion this engine does not follow; (3) a guard \
         that lives on the ROUTER the route is mounted under rather than on the route (a tRPC \
         `authedAdminProcedure`, an Express router-level `.use(..)` this engine's `router_mounts` \
         producer did not resolve). None of the three is detected here, so this range holds whether or \
         not this tree uses them. Close it by injecting \
         `auth-guarded` for the guarded routes (a whole-tree envelope projection's own `attributes`, \
         Mode A; or an `adapterOverlays` overlay on a natively-parsed tree, Mode B), by marking a vetted \
         route \
         with `// zzop-protected-path-no-auth-evidence-ok` on its registration line, or by turning the \
         rule off. Reviewing the reported routes one by one is worth it either way: on the tree that \
         produced this disclosure's shape, the composite decorator appeared on both guarded AND \
         deliberately public routes, so its presence alone does not settle a route.",
        examples.join(", "),
    ))
}

#[cfg(test)]
mod tests;
