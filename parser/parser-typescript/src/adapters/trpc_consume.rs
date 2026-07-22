//! tRPC CONSUME extractor — projects the client-side tRPC procedure calls a TS/JS tree CONSUMES, so
//! the core cross-layer linker can join each call to its router-side handler (`trpc_router` projects
//! the PROVIDE side). Join key: `(kind="trpc", key="VERB dotted.procedure.path")` — exact string
//! match, same contract shape as `egress`'s `http` kind. tRPC's client-call shapes
//! (`useQuery`/`useMutation`/proxy-client `.query()`/`.mutate()`/...) are vendor vocabulary, like
//! `egress`'s `axios`/`ky`/`fetch` family — a fixed, closed set the client libraries themselves emit.
//!
//! A "client identifier" is a local binding recognized via either: a **local factory call**
//! (`const trpc = createTRPCReact<AppRouter>();`, also `createTRPCNext`/`createTRPCProxyClient`/
//! `createTRPCClient`/`createTRPCOptionsProxy`/... — matched by callee ident PREFIX `createTRPC`,
//! initializer unwrapped via `unwrap_expr` first), or an **import specifier** containing `trpc`
//! case-insensitively (`@acme/trpc/react`, `~/utils/trpc`, `@trpc/client`, ...). Both routes only
//! collect BINDING NAMES — a whole-file heuristic like every other extractor here, not real scoping.
//!
//! ## Chain matching
//! A call whose callee is `client.seg1. ... .segN.<terminal>(...)`, `client` a recognized client
//! identifier, every segment a plain identifier member access, `N >= 1`. Terminal verb table:
//! - `useQuery`/`useSuspenseQuery`/`useInfiniteQuery`/`useSuspenseInfiniteQuery`/`usePrefetchQuery`/
//!   `useQueries`/`query`/`fetch` -> `QUERY`
//! - `useMutation`/`mutate`/`mutateAsync` -> `MUTATION`
//! - `useSubscription`/`subscribe` -> `SUBSCRIPTION`
//!
//! A computed segment anywhere in the chain makes the WHOLE chain dynamic and skips it, honestly.
//! Also skipped: `useUtils()`/`.invalidate(...)` cache-plumbing and server-side `createCaller(...)`
//! calls. Emission order is visitor (AST) order, i.e. source order.

use std::collections::HashSet;

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{CallExpr, Callee, Expr, MemberProp, Pat, VarDeclarator};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::IoConsume;

/// Extract tRPC client-call CONSUME entries from one file.
pub fn extract_trpc_consumes(rel: &str, text: &str) -> Vec<IoConsume> {
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };

    let mut clients: HashSet<String> = HashSet::new();
    for (local, binding) in crate::parse_imports(rel, text) {
        if binding.specifier.to_ascii_lowercase().contains("trpc") {
            clients.insert(local);
        }
    }
    let mut factory = FactoryClientCollector {
        idents: HashSet::new(),
    };
    module.visit_with(&mut factory);
    clients.extend(factory.idents);

    let cm_ref: &SourceMap = &cm;
    let mut collector = ConsumeCollector {
        cm: cm_ref,
        file: rel,
        clients: &clients,
        out: Vec::new(),
    };
    module.visit_with(&mut collector);
    collector.out
}

/// Collects local bindings initialized by a `createTRPC*(...)` call — e.g.
/// `const trpc = createTRPCReact<AppRouter>();`.
struct FactoryClientCollector {
    idents: HashSet<String>,
}

impl Visit for FactoryClientCollector {
    fn visit_var_declarator(&mut self, d: &VarDeclarator) {
        if let (Pat::Ident(bi), Some(init)) = (&d.name, &d.init) {
            if let Expr::Call(call) = unwrap_expr(init) {
                if let Callee::Expr(callee) = &call.callee {
                    if let Expr::Ident(id) = &**callee {
                        if id.sym.starts_with("createTRPC") {
                            self.idents.insert(bi.id.sym.to_string());
                        }
                    }
                }
            }
        }
        d.visit_children_with(self);
    }
}

struct ConsumeCollector<'a> {
    cm: &'a SourceMap,
    file: &'a str,
    clients: &'a HashSet<String>,
    out: Vec<IoConsume>,
}

impl Visit for ConsumeCollector<'_> {
    fn visit_call_expr(&mut self, call: &CallExpr) {
        if let Some((verb, path)) = match_trpc_call(call, self.clients) {
            self.out.push(IoConsume {
                client: None,
                body: None,
                kind: "trpc".into(),
                key: Some(format!("{verb} {path}")),
                file: self.file.into(),
                line: crate::line_of(self.cm, call.span.lo),
                raw: None,
                method: None,
                retry_configured: None,
            });
        }
        call.visit_children_with(self); // recurse into nested calls
    }
}

/// Matches `client.seg1. ... .segN.<terminal>(...)` against `clients`, returning
/// `(VERB, "seg1.....segN")` — `None` on any gate failure (see module doc).
fn match_trpc_call(call: &CallExpr, clients: &HashSet<String>) -> Option<(&'static str, String)> {
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Member(outer) = unwrap_expr(callee) else {
        return None;
    };
    let MemberProp::Ident(terminal) = &outer.prop else {
        return None;
    };
    let verb = terminal_verb(terminal.sym.as_ref())?;
    let (root, segs) = collect_chain(&outer.obj)?;
    if segs.is_empty() || !clients.contains(&root) {
        return None;
    }
    Some((verb, segs.join(".")))
}

/// Walks a member-access expression to its root identifier, collecting intermediate plain-ident
/// segments in source order. `None` the instant a computed (`[...]`) segment is hit, or the root
/// isn't a plain identifier.
fn collect_chain(expr: &Expr) -> Option<(String, Vec<String>)> {
    match unwrap_expr(expr) {
        Expr::Ident(id) => Some((id.sym.to_string(), Vec::new())),
        Expr::Member(m) => {
            let MemberProp::Ident(name) = &m.prop else {
                return None;
            };
            let (root, mut segs) = collect_chain(&m.obj)?;
            segs.push(name.sym.to_string());
            Some((root, segs))
        }
        _ => None,
    }
}

/// The tRPC client-call terminal -> CONSUME verb table — see this module's doc for the full rationale.
fn terminal_verb(name: &str) -> Option<&'static str> {
    match name {
        "useQuery"
        | "useSuspenseQuery"
        | "useInfiniteQuery"
        | "useSuspenseInfiniteQuery"
        | "usePrefetchQuery"
        | "useQueries"
        | "query"
        | "fetch" => Some("QUERY"),
        "useMutation" | "mutate" | "mutateAsync" => Some("MUTATION"),
        "useSubscription" | "subscribe" => Some("SUBSCRIPTION"),
        _ => None,
    }
}

/// Strip wrappers between an expression and its real value: `... as const`, `(...)`, `... satisfies T`, `...!`.
/// Copy of `egress::unwrap_expr` (that one is private to its own module).
fn unwrap_expr(e: &Expr) -> &Expr {
    let mut n = e;
    loop {
        n = match n {
            Expr::TsAs(a) => &a.expr,
            Expr::TsConstAssertion(c) => &c.expr,
            Expr::Paren(p) => &p.expr,
            Expr::TsSatisfies(s) => &s.expr,
            Expr::TsNonNull(nn) => &nn.expr,
            other => return other,
        };
    }
}

#[cfg(test)]
mod tests {
    //! Coverage: both client-detection routes, the three verb families, and the honest-skip cases
    //! (non-client ident, no middle segments, computed segment).
    use super::*;

    fn keys(out: &[IoConsume]) -> Vec<Option<String>> {
        out.iter().map(|c| c.key.clone()).collect()
    }

    #[test]
    fn import_based_client_use_query() {
        let out = extract_trpc_consumes(
            "a.tsx",
            "import { trpc } from \"@acme/trpc/react\";\ntrpc.viewer.bookings.get.useQuery({});",
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "trpc");
        assert_eq!(out[0].key.as_deref(), Some("QUERY viewer.bookings.get"));
        assert_eq!(out[0].file, "a.tsx");
        assert_eq!(out[0].line, 2);
        assert!(out[0].raw.is_none());
        assert!(out[0].method.is_none());
    }

    #[test]
    fn local_create_trpc_react_client_use_mutation() {
        let out = extract_trpc_consumes(
            "b.tsx",
            "const trpc = createTRPCReact<AppRouter>();\ntrpc.viewer.apiKeys.create.useMutation();",
        );
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].key.as_deref(),
            Some("MUTATION viewer.apiKeys.create")
        );
        assert_eq!(out[0].line, 2);
    }

    #[test]
    fn proxy_client_query_and_mutate_terminals() {
        let out = extract_trpc_consumes(
            "c.ts",
            "const client = createTRPCProxyClient<AppRouter>();\nclient.admin.listReports.query();\nclient.admin.dismiss.mutate({});",
        );
        assert_eq!(
            keys(&out),
            vec![
                Some("QUERY admin.listReports".to_string()),
                Some("MUTATION admin.dismiss".to_string()),
            ]
        );
        assert_eq!(out[0].line, 2);
        assert_eq!(out[1].line, 3);
    }

    #[test]
    fn non_client_ident_with_same_chain_shape_is_not_emitted() {
        let out = extract_trpc_consumes(
            "d.ts",
            "import lodash from \"lodash\";\nlodash.a.b.useQuery();",
        );
        assert!(out.is_empty());
    }

    #[test]
    fn bare_call_with_no_middle_segments_is_not_emitted() {
        let out = extract_trpc_consumes(
            "e.tsx",
            "import { trpc } from \"@acme/trpc/react\";\ntrpc.useQuery();",
        );
        assert!(out.is_empty());
    }

    #[test]
    fn computed_segment_is_skipped() {
        let out = extract_trpc_consumes(
            "f.tsx",
            "import { trpc } from \"@acme/trpc/react\";\ntrpc.viewer[x].get.useQuery();",
        );
        assert!(out.is_empty());
    }

    #[test]
    fn multiple_consumes_in_one_file_are_in_source_order() {
        let out = extract_trpc_consumes(
            "g.tsx",
            "import { trpc } from \"@acme/trpc/react\";\ntrpc.viewer.bookings.get.useQuery({});\ntrpc.viewer.apiKeys.create.useMutation();",
        );
        assert_eq!(
            keys(&out),
            vec![
                Some("QUERY viewer.bookings.get".to_string()),
                Some("MUTATION viewer.apiKeys.create".to_string()),
            ]
        );
        assert_eq!(out[0].line, 2);
        assert_eq!(out[1].line, 3);
    }

    #[test]
    fn subscription_terminal_is_recognized() {
        let out = extract_trpc_consumes(
            "h.tsx",
            "import { trpc } from \"@acme/trpc/react\";\ntrpc.viewer.notifications.useSubscription();",
        );
        assert_eq!(
            out[0].key.as_deref(),
            Some("SUBSCRIPTION viewer.notifications")
        );
    }
}
