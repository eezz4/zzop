//! `route-shadowing` — within one file's HTTP `provides`, a param-segment route registered at an EARLIER
//! line than a literal-segment route of the same shape shadows it in a first-match router (Express/Koa/
//! NestJS-style, "the first registered pattern that matches wins"): the param route also matches every
//! request the literal route was meant to catch, making the literal handler unreachable in practice.
//!
//! Two `IoProvide`s (`kind == "http"`) shadow each other only when: same `file` (cross-file pairs carry no
//! registration-order signal) and method; same segment count in the normalized `http_interface_key` path;
//! every segment identical except one, where the earlier route has a `{}` placeholder and the later route
//! has a literal there; and the param route's `line` is strictly less than the literal route's. A position
//! where both are literal is `duplicate-route`'s territory. When several earlier param routes qualify, the
//! EARLIEST is reported, since it intercepts first in a first-match router.
//!
//! "First registered pattern wins" is an Express/Koa/NestJS convention, not universal — a router that
//! picks the MOST-SPECIFIC match regardless of order never exhibits this shadow (a literal always beats a
//! same-shape param there). Rather than emit a known false positive on those frameworks and rely on the
//! reader to disable it, this check is FRAMEWORK-SCOPED by file extension: it runs only on the first-match
//! ecosystems zzop extracts — see [`FIRST_MATCH_ROUTER_EXTENSIONS`]. It stays a Warning (never Critical)
//! and the precision caveat remains in the message for the residual (a first-match language whose specific
//! framework happens to be specificity-matched).

/// File extensions of the frameworks zzop extracts that use FIRST-MATCH routing — registration order
/// decides which pattern wins, the only semantics under which an earlier param route shadows a later
/// literal one. The TypeScript/JavaScript ecosystem (Express/Koa/Hono/NestJS) and Python FastAPI
/// (Starlette matches in registration order — its own docs warn to declare a fixed `/users/me` before
/// `/users/{id}`). Frameworks that pick the MOST-SPECIFIC match regardless of order — Java Spring
/// (`AntPathMatcher`), C# ASP.NET routing, Go gin/`net/http`, Rust axum — are exempt: a literal always
/// beats a same-shape param there, so the shadow cannot occur. Positive allowlist, mirroring
/// `mutating_route_no_auth::CALL_GRAPH_COVERED_EXTENSIONS`: a language not yet known to be first-match
/// defaults to EXEMPT (no false shadow) until its routing semantics are pinned here. RESIDUAL: the gate is
/// per-EXTENSION, so a `.ts`/`.js` app whose specific router is itself specificity-match (Fastify's radix
/// `find-my-way`) can't be told apart from Express and still fires — the message's precision caveat covers
/// that tail; a per-framework signal would be needed to close it.
pub const FIRST_MATCH_ROUTER_EXTENSIONS: &[&str] = &[
    "ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts", "py", "pyi",
];

/// True when `file`'s extension is a first-match-router language — see [`FIRST_MATCH_ROUTER_EXTENSIONS`].
fn is_first_match_router(file: &str) -> bool {
    std::path::Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| FIRST_MATCH_ROUTER_EXTENSIONS.contains(&e.as_str()))
}

pub fn route_shadowing_findings(io_provides: &[zzop_core::IoProvide]) -> Vec<zzop_core::Finding> {
    let mut by_file_method: std::collections::BTreeMap<(&str, &str), Vec<&zzop_core::IoProvide>> =
        std::collections::BTreeMap::new();
    for p in io_provides {
        if p.kind != "http" {
            continue;
        }
        // Order-dependent shadowing is a first-match-router concept only — a specificity-match framework
        // (Spring/ASP.NET/gin/axum) picks the literal regardless of order, so its routes never shadow.
        if !is_first_match_router(&p.file) {
            continue;
        }
        let Some((method, _path)) = p.key.split_once(' ') else {
            continue;
        };
        by_file_method
            .entry((p.file.as_str(), method))
            .or_default()
            .push(p);
    }

    let mut findings = Vec::new();
    for ((_, _), mut routes) in by_file_method {
        routes.sort_by_key(|p| p.line);
        for (i, literal) in routes.iter().enumerate() {
            let Some((_, literal_path)) = literal.key.split_once(' ') else {
                continue;
            };
            let literal_segs: Vec<&str> = literal_path.split('/').collect();
            let mut earliest_shadow: Option<&zzop_core::IoProvide> = None;
            for cand in &routes[..i] {
                let Some((_, cand_path)) = cand.key.split_once(' ') else {
                    continue;
                };
                let cand_segs: Vec<&str> = cand_path.split('/').collect();
                if !shadows(&cand_segs, &literal_segs) {
                    continue;
                }
                if earliest_shadow.is_none_or(|cur| cand.line < cur.line) {
                    earliest_shadow = Some(cand);
                }
            }
            if let Some(param) = earliest_shadow {
                findings.push(zzop_core::Finding {
                    rule_id: "route-shadowing".to_string(),
                    severity: zzop_core::Severity::Warning,
                    file: literal.file.clone(),
                    line: literal.line,
                    message: format!(
                        "Route `{}` (registered here at line {}) is shadowed by an earlier param route `{}` \
                         registered at line {} in the same file — in a first-match router (Express/Koa/\
                         NestJS-style), the param route's pattern also matches every request this literal \
                         route was meant to catch, so the earlier registration intercepts first and this \
                         handler is effectively unreachable. Fix: register the literal route BEFORE the param \
                         route (or merge them into one handler that branches on the concrete value). Precision \
                         limit: \"first registered pattern wins\" is framework-dependent — a router that picks \
                         the most-specific match regardless of registration order is unaffected by this shape; \
                         disable {} if that's your framework or the ordering is intentional (this rule has no \
                         inline suppression marker).",
                        literal.key,
                        literal.line,
                        param.key,
                        param.line,
                        // `disable_hint` itself always starts with "Disable " — this site's surrounding
                        // sentence already supplies "disable" (mid-sentence, after a semicolon), so only the
                        // "via config ..." remainder is spliced in, same technique
                        // `rules-schema/src/message.rs`'s `disable_hint_tail` uses.
                        zzop_core::disable_hint("route-shadowing")
                            .strip_prefix("Disable ")
                            .expect("disable_hint always starts with \"Disable \"")
                    ),
                    data: Some(serde_json::json!({
                        "literalKey": literal.key,
                        "literalLine": literal.line,
                        "paramKey": param.key,
                        "paramLine": param.line,
                        "file": literal.file,
                    })),
                });
            }
        }
    }
    findings.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    findings
}

/// True when `param_segs` (the earlier route) and `literal_segs` (the later route) are the same shape
/// except for exactly one position, where `param_segs` holds `{}` and `literal_segs` holds a literal there.
fn shadows(param_segs: &[&str], literal_segs: &[&str]) -> bool {
    if param_segs.len() != literal_segs.len() {
        return false;
    }
    let mut diff_at_param_placeholder = false;
    for (a, b) in param_segs.iter().zip(literal_segs.iter()) {
        if a == b {
            continue;
        }
        if diff_at_param_placeholder {
            return false; // a second differing position — not the decidable subset
        }
        if *a != "{}" || *b == "{}" {
            return false; // the differing position must be param-vs-literal, not literal-vs-literal
        }
        diff_at_param_placeholder = true;
    }
    diff_at_param_placeholder
}

#[cfg(test)]
mod tests;
