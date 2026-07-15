//! Per-file IO projection fused into the parse pass: HTTP egress (`consumes`, frontend side) and
//! Hono-style route provides (`provides`, backend side), via `zzop-parser-typescript`'s `egress`/`routes`
//! adapters, run against a single-file slice (`pipeline::process_file` calls [`extract_file_io`] once
//! per file, before that file's parse scratch state is dropped).
//!
//! ## tRPC
//! `extract_file_io` also folds in tRPC client-call consumes (kind `"trpc"`), already fully keyed at
//! extraction time. The provide side is NOT projected here: a tRPC router's full route path is only
//! knowable once every file's router fragment is assembled (e.g. a `viewerRouter` mounting a
//! `bookingsRouter` imported from another file). Instead `pipeline::FileArtifact::
//! procedure_router_fragments` collects each file's own router shape, and `analyze::compose_trpc_provides`
//! composes every fragment at assembly time.
//!
//! ## Cross-file resolution (fragment now, compose later)
//! Both TS-side adapters were designed for a project-wide call, so cross-file indirection does not
//! resolve at this one-file call site:
//! - `extract_http_egress`: a file-local constant still resolves (`build_const_map` runs over the same
//!   slice). A cross-file constant falls through to `IoConsume { key: None, raw: Some(<expr>), method:
//!   Some(<METHOD>) }`. [`extract_file_io`] also collects this file's own constant-map fragment
//!   (`const_map_fragment`) into `FileArtifact::const_map_fragment`; `analyze::assemble` merges every
//!   file's fragment into one project-wide map and `analyze::late_resolve_cross_file_consumes`
//!   re-resolves the unresolved consumes against it before `MinimalIr::io` is frozen. A genuinely
//!   dynamic call, or a constant assigned via `Object.assign`/spread rather than a plain literal, stays
//!   honestly unresolved.
//! - Code-registered routers (Hono-style): this per-file pass derives no router provides at all;
//!   `pipeline::compute_fresh_artifact` projects each file's own router-mount shape
//!   (`FileArtifact::router_mount_fragments`) and `analyze::compose_router_mount_provides` joins them —
//!   chained builders, cross-file mounts, mount prefixes — into whole-tree `http` provides.

use zzop_core::{IoConsume, IoFacts, IoProvide};

/// Config for the fused per-file pass's BE route adapter. Route file *paths* aren't a separate config
/// concern: under per-file fusion every file is its own sole candidate (see [`extract_file_io`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoOptions {
    /// Identifier names treated as a Hono-style router (`<name>.get(...)`, `<name>.route(...)`)
    /// by the router-mount fragment recognizer. Default: `["apiRoutes"]`.
    pub router_names: Vec<String>,
}

impl Default for IoOptions {
    fn default() -> Self {
        IoOptions {
            router_names: vec!["apiRoutes".to_string()],
        }
    }
}

/// Projects one Java file's `IoFacts` — Spring MVC HTTP route provides only (`consumes` is always empty;
/// this engine has no Java-side HTTP-egress extractor yet). Delegates to
/// `zzop_parser_java::extract_http_provides` — see that function's module doc for the annotation shapes
/// recognized and the `@RestController`/`@Controller` class-gating rule. `None` when the file yields no
/// provides at all. Called only for `.java` files (`Language::JavaLexical`).
pub(crate) fn extract_java_file_io(rel: &str, text: &str) -> Option<IoFacts> {
    let provides = zzop_parser_java::extract_http_provides(rel, text);
    if provides.is_empty() {
        None
    } else {
        Some(IoFacts {
            provides,
            consumes: Vec::new(),
        })
    }
}

/// Projects one file's `IoFacts` (HTTP/tRPC egress it consumes + NestJS controller routes it
/// provides), or `None` when no adapter found anything. Called only for well-formed, in-size-cap
/// TypeScript files (`pipeline::process_file`). Code-registered router provides (Hono-style) are NOT
/// projected here — they travel as `FileArtifact::router_mount_fragments` and compose whole-tree in
/// `analyze` (module doc).
///
/// The controller-decorator adapter (`zzop_parser_typescript::extract_controller_provides`) stays
/// per-file because a NestJS- or `@n8n/decorators`-style route decorator is entirely self-contained
/// within one file's own class/method AST — there is no cross-file indirection to resolve.
pub(crate) fn extract_file_io(rel: &str, text: &str, opts: &IoOptions) -> Option<IoFacts> {
    let files = [(rel.to_string(), text.to_string())];

    let mut consumes: Vec<IoConsume> = zzop_parser_typescript::extract_http_egress(&files);
    // tRPC client-call consumes (kind "trpc"): already fully keyed at extraction time, so no
    // late-resolution pass is needed for this kind.
    consumes.extend(zzop_parser_typescript::extract_trpc_consumes(rel, text));
    // Hono client typed-RPC consumes (kind "http"): keyed when the client's base path is statically
    // resolvable; an unresolvable base falls back to the same unresolved shape as egress's dynamic-URL
    // case.
    consumes.extend(zzop_parser_typescript::extract_hono_client_consumes(
        rel, text,
    ));
    // db-table consumes (kind "db-table"): a Prisma `getPrisma().<model>.<method>(...)` access, keyed at
    // extraction time. Feeds the generic cross-layer linker so `cross-layer/shared-db-table` can fire when
    // 2+ trees touch one table. See decisions/2026-07-rule-side-lexical-reparse-leak.md.
    consumes.extend(zzop_parser_typescript::extract_db_table_consumes(rel, text));
    // `axios.defaults.baseURL = "literal"` sentinel (kind "client-base-prefix"): a tree-level
    // axios base-path marker consumed and stripped by `analyze::assemble` after late cross-file
    // resolution (see `client_base.rs`'s module doc) — this per-file pass only surfaces it.
    consumes.extend(zzop_parser_typescript::extract_client_base_prefix_marker(
        rel, text,
    ));

    // Code-registered router provides (Hono-style) come from `FileArtifact::router_mount_fragments`
    // instead (module doc). `opts.router_names` is consumed by that fragment projection, not here.
    let _ = opts;
    let mut provides: Vec<IoProvide> =
        zzop_parser_typescript::extract_controller_provides(rel, text);
    // NestJS `app.setGlobalPrefix('api')` sentinel (kind "nest-global-prefix"): a project-level marker
    // consumed and stripped by `analyze::assemble` once every file's `IoFacts` are aggregated tree-wide
    // (see `global_prefix.rs`'s module doc) — this per-file pass only needs to surface it.
    provides.extend(zzop_parser_typescript::extract_global_prefix_marker(
        rel, text,
    ));
    // Manual pathname-dispatch route provides (framework-less Workers/Node servers): like the
    // controller-decorator adapter, the whole dispatch shape is self-contained in one file's own
    // AST (the compared path is a literal), so it projects per-file with no fragment to compose.
    provides.extend(zzop_parser_typescript::extract_pathname_dispatch_provides(
        rel, text,
    ));

    if provides.is_empty() && consumes.is_empty() {
        None
    } else {
        Some(IoFacts { provides, consumes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> IoOptions {
        IoOptions::default()
    }

    #[test]
    fn no_io_in_a_plain_file_is_none() {
        assert!(extract_file_io("a.ts", "export const a = 1;\n", &opts()).is_none());
    }

    #[test]
    fn captures_fe_http_egress_consume() {
        let io = extract_file_io("Ctx.tsx", r#"axios.get("/authen/getUserInfo");"#, &opts())
            .expect("expected io facts");
        assert!(io.provides.is_empty());
        assert_eq!(io.consumes.len(), 1);
        assert_eq!(
            io.consumes[0].key.as_deref(),
            Some("GET /authen/getUserInfo")
        );
        assert_eq!(io.consumes[0].file, "Ctx.tsx");
    }

    #[test]
    fn file_local_constant_indirection_still_resolves() {
        let src = r#"const ControlKey = { AUTHEN: { getUserInfo: "/authen/getUserInfo" } };
axios.get(ControlKey.AUTHEN.getUserInfo);"#;
        let io = extract_file_io("Ctx.tsx", src, &opts()).expect("expected io facts");
        assert_eq!(
            io.consumes[0].key.as_deref(),
            Some("GET /authen/getUserInfo")
        );
    }

    #[test]
    fn cross_file_constant_indirection_is_unresolved_at_this_one_file_call_site() {
        // Same indirection shape as egress.rs's own test, but the constant lives in a different file
        // this one-file-slice call never sees — see module doc. `analyze::late_resolve_cross_file_consumes`
        // resolves this shape end to end (see lib.rs's e2e test).
        let io = extract_file_io(
            "Ctx.tsx",
            "axios.get(ControlKey.AUTHEN.getUserInfo);",
            &opts(),
        )
        .expect("expected io facts (unresolved consume is still reported)");
        assert_eq!(io.consumes.len(), 1);
        assert!(io.consumes[0].key.is_none());
        assert_eq!(io.consumes[0].method.as_deref(), Some("GET"));
        assert_eq!(
            io.consumes[0].raw.as_deref(),
            Some("ControlKey.AUTHEN.getUserInfo")
        );
    }

    #[test]
    fn hono_route_provides_no_longer_come_from_the_per_file_pass() {
        // Router provides now come from the fragment-then-compose pipeline (module doc), not this
        // per-file pass — a Hono file with no egress/Nest facts yields nothing here.
        let src = "const apiRoutes = new Hono();\napiRoutes.get(\"/users\", api.listUsers);\n";
        assert!(extract_file_io("routes/apiRoutes.ts", src, &opts()).is_none());
    }

    #[test]
    fn captures_nestjs_controller_route_provide_through_the_fused_seam() {
        let src =
            "@Controller('users')\nclass UsersController {\n  @Get(':id')\n  findOne() {}\n}\n";
        let io = extract_file_io("users.controller.ts", src, &opts()).expect("expected io facts");
        assert!(io.consumes.is_empty());
        assert_eq!(io.provides.len(), 1);
        assert_eq!(io.provides[0].key, "GET /users/{}");
        assert_eq!(io.provides[0].line, 3);
        assert_eq!(io.provides[0].symbol.as_deref(), Some("findOne"));
    }

    #[test]
    fn captures_nest_global_prefix_marker_through_the_fused_seam() {
        let src = "app.setGlobalPrefix('api');\n";
        let io = extract_file_io("main.ts", src, &opts()).expect("expected io facts");
        assert!(io.consumes.is_empty());
        assert_eq!(io.provides.len(), 1);
        assert_eq!(io.provides[0].kind, "nest-global-prefix");
        assert_eq!(io.provides[0].key, "api");
    }

    #[test]
    fn captures_hono_client_consume_through_the_fused_seam() {
        let src = "import { hc } from 'hono/client';\nconst client = hc<T>('/api/auth');\nclient.signout.$post();\n";
        let io = extract_file_io("client.ts", src, &opts()).expect("expected io facts");
        assert!(io.provides.is_empty());
        assert_eq!(io.consumes.len(), 1);
        assert_eq!(io.consumes[0].kind, "http");
        assert_eq!(
            io.consumes[0].key.as_deref(),
            Some("POST /api/auth/signout")
        );
        assert_eq!(io.consumes[0].file, "client.ts");
    }

    // --- extract_java_file_io ---

    #[test]
    fn no_java_io_in_a_plain_class_is_none() {
        assert!(extract_java_file_io("C.java", "class C {}\n").is_none());
    }

    #[test]
    fn captures_spring_get_mapping_provide_with_no_consumes() {
        let src = "@RestController\nclass CtrlAuthen {\n  @GetMapping(\"/getUserInfo\")\n  UserInfo getUserInfo() { return null; }\n}\n";
        let io = extract_java_file_io("CtrlAuthen.java", src).expect("expected io facts");
        assert!(io.consumes.is_empty());
        assert_eq!(io.provides.len(), 1);
        assert_eq!(io.provides[0].key, "GET /getUserInfo");
        assert_eq!(io.provides[0].symbol.as_deref(), Some("getUserInfo"));
    }
}
