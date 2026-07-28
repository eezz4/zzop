//! Per-file IO projection fused into the parse pass: HTTP egress (`consumes`, frontend side) and Hono-style route provides
//! (`provides`, backend side), via `zzop-parser-typescript`'s `egress`/`routes` adapters, run against a single-file slice
//! (`pipeline::process_file` calls [`extract_file_io`] once per file, before that file's parse scratch state is dropped).
//! ## tRPC
//! `extract_file_io` also folds in tRPC client-call consumes (kind `"trpc"`), already fully keyed at extraction time. The provide
//! side is NOT projected here: a tRPC router's full route path is only knowable once every file's router fragment is assembled
//! (e.g. a `viewerRouter` mounting a `bookingsRouter` imported from another file). Instead `pipeline::FileArtifact::
//! procedure_router_fragments` collects each file's own router shape, and `analyze::compose_trpc_provides` composes every fragment at assembly time.
//! ## Cross-file resolution (fragment now, compose later)
//! Both TS-side adapters were designed for a project-wide call, so cross-file indirection does not resolve at this one-file call site:
//! - `extract_http_egress`: a file-local constant still resolves (`build_const_map` runs over the same slice). A cross-file constant
//!   falls through to `IoConsume { key: None, raw: Some(<expr>), method: Some(<METHOD>) }`. [`extract_file_io`] also collects this file's
//!   own constant-map fragment (`const_map_fragment`) into `FileArtifact::const_map_fragment`; `analyze::assemble` merges every file's
//!   fragment into one project-wide map and `analyze::late_resolve_cross_file_consumes` re-resolves the unresolved consumes against it
//!   before `MinimalIr::io` is frozen. A genuinely dynamic call, or a constant assigned via `Object.assign`/spread rather than a plain literal, stays honestly unresolved.
//! - Code-registered routers (Hono-style): this per-file pass derives no router provides at all;
//!   `pipeline::compute_fresh_artifact` projects each file's own router-mount shape (`FileArtifact::router_mount_fragments`) and
//!   `analyze::compose_router_mount_provides` joins them — chained builders, cross-file mounts, mount prefixes — into whole-tree `http` provides.

use zzop_core::{IoConsume, IoFacts, IoProvide};

/// The router identifiers this recognizer assumes when a project declares none — the value
/// `vocabulary.routerNames` replaces, and the ONE place the name lives (`VocabularyConfig::built_in`
/// reads this symbol rather than re-spelling it, the same T1 single-definition rule
/// `zzop_cache::DEFAULT_CACHE_DIR` follows).
///
/// It was an inline `vec!["apiRoutes"]` inside `IoOptions::default` until 2026-07-27, and that form is
/// why it went unnoticed for so long: the policy-value census reads `const` declarations, so a name
/// vocabulary written as a struct-field default was invisible to it AND unreachable from any config —
/// an "escape hatch" (this type's own words) with no door. Naming it fixes both halves at once.
pub const DEFAULT_ROUTER_NAMES: &[&str] = &["apiRoutes"];

/// Config for the fused per-file pass's BE route adapter. Route file *paths* aren't a separate config
/// concern: under per-file fusion every file is its own sole candidate (see [`extract_file_io`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoOptions {
    /// Escape-hatch identifier allowlist for the router-mount recognizer — vocabulary-agnostic ROUTE rules only (`.get/.post/.put/.patch/.delete/.all`, `.route()`), not just Hono-style.
    /// It does NOT confer `.use()` mounting: that is gated on `is_express`, which only the AST recognizer sets (`express()`/`express.Router()`/an imported `Router`). Declared through `vocabulary.routerNames`; [`DEFAULT_ROUTER_NAMES`] is what `zzop init` writes.
    pub router_names: Vec<String>,
}

impl Default for IoOptions {
    fn default() -> Self {
        IoOptions {
            router_names: DEFAULT_ROUTER_NAMES.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// Projects one Java file's `IoFacts` — Spring MVC HTTP route provides only (`consumes` is always empty; this engine has no
/// Java-side HTTP-egress extractor yet). Delegates to `zzop_parser_java_21::extract_http_provides` — see that function's module doc
/// for the annotation shapes recognized and the `@RestController`/`@Controller` class-gating rule. `None` when the file yields no provides at all. Called only for `.java` files (`Language::Java21`).
pub(crate) fn extract_java_file_io(rel: &str, text: &str) -> Option<IoFacts> {
    let provides = zzop_parser_java_21::extract_http_provides(rel, text);
    if provides.is_empty() {
        None
    } else {
        Some(IoFacts {
            provides,
            consumes: Vec::new(),
        })
    }
}

/// Projects one C# file's `IoFacts` — ASP.NET Core attribute-controller + minimal-API HTTP route provides AND `HttpClient` literal
/// HTTP egress consumes. C# is the one native language whose io projection carries BOTH directions here directly (unlike
/// Go/Rust/Python, whose route provides travel as `router_mount_fragments` and compose whole-tree). `None` when the file yields neither.
///
/// Called for EVERY `.cs` file. Route PROVIDES project regardless of `degraded` (Java-parity — like `extract_java_file_io`,
/// `extract_csharp_http_provides` returns nothing rather than guessing on a malformed file, so a controller with one
/// syntactically-broken sibling method still contributes its well-formed routes). Egress CONSUMES stay gated behind `!degraded`,
/// matching every other language's consume arm (`Rust`/`Go`/`Python` in `pipeline::fresh`) — a degraded parse can't be trusted to
/// have seen the whole call site. Before this split, both directions were dropped for any degraded `.cs` file, silently vanishing real endpoints from the cross-layer join.
pub(crate) fn extract_csharp_file_io(rel: &str, text: &str, degraded: bool) -> Option<IoFacts> {
    let provides = zzop_parser_csharp::extract_csharp_http_provides(rel, text);
    let consumes = if degraded {
        Vec::new()
    } else {
        zzop_parser_csharp::extract_csharp_http_consumes(rel, text)
    };
    (!provides.is_empty() || !consumes.is_empty()).then_some(IoFacts { provides, consumes })
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
pub(crate) fn extract_file_io(
    rel: &str,
    text: &str,
    opts: &IoOptions,
    vocab: &crate::vocabulary::ResolvedVocabulary<'_>,
) -> Option<IoFacts> {
    let files = [(rel.to_string(), text.to_string())];

    let mut consumes: Vec<IoConsume> =
        zzop_parser_typescript::extract_http_egress_with_vocab(&files, &vocab.retry_wrappers);
    // tRPC client-call consumes (kind "trpc"): already fully keyed at extraction time, so no
    // late-resolution pass is needed for this kind.
    consumes.extend(zzop_parser_typescript::extract_trpc_consumes(rel, text));
    // Hono client typed-RPC consumes (kind "http"): keyed when the client's base path is statically
    // resolvable; an unresolvable base falls back to the same unresolved shape as egress's dynamic-URL
    // case.
    consumes.extend(zzop_parser_typescript::extract_hono_client_consumes(
        rel, text,
    ));
    // db-table consumes (kind "db-table"): a Prisma `getPrisma().<model>` or bare `prisma.<model>`
    // access, keyed at extraction time in the PARSER (not re-lexed in a rule) — io facts project during
    // parsing, before AST drop. Feeds the linker so `cross-layer/db-table-name-in-multiple-sources` fires across trees.
    consumes.extend(
        zzop_parser_typescript::extract_db_table_consumes_with_vocab(
            rel,
            text,
            vocab.prisma_client_getter,
        ),
    );
    // Raw-SQL db-table consumes (kind "db-table"): the table names a SQL statement STRING in this file
    // reads or writes, keyed at extraction time through `zzop_parser_sql` — the ORM-less arm of the same
    // channel, for stacks (Cloudflare D1, better-sqlite3, pg, mysql2) whose tables appear only inside
    // strings and are therefore invisible to every ORM-symbol recognizer above.
    consumes.extend(zzop_parser_typescript::extract_raw_sql_db_table_consumes(
        rel, text,
    ));
    // TypeORM repository-access consumes (kind "db-table", key None, raw = entity class): unkeyable at
    // parse time; `analyze::assemble::resolve_orm_entity_consumes` keys them from the entity index.
    consumes.extend(zzop_parser_typescript::extract_typeorm_repository_consumes(
        rel, text,
    ));
    // `axios.defaults.baseURL = "literal"` sentinel (kind "client-base-prefix"): a tree-level
    // axios base-path marker consumed and stripped by `analyze::assemble` after late cross-file
    // resolution (see `client_base.rs`'s module doc) — this per-file pass only surfaces it.
    consumes.extend(zzop_parser_typescript::extract_client_base_prefix_marker(
        rel, text,
    ));
    // Same sentinel tagged `client: "generated"`: the swagger `HttpClient` base field; the
    // client-generic `apply_client_base_prefixes` prefixes `client == "generated"` consumes too.
    consumes.extend(zzop_parser_typescript::extract_generated_client_base_prefix_marker(rel, text));

    // Code-registered router provides (Hono-style) come from `FileArtifact::router_mount_fragments`
    // instead (module doc). `opts.router_names` is consumed by that fragment projection, not here.
    let _ = opts;
    let mut provides: Vec<IoProvide> =
        zzop_parser_typescript::extract_controller_provides(rel, text);
    // TypeORM `@Entity('table_name')` class decorator (kind "db-table"): joins the same cross-layer
    // channel a Prisma PSL model / SQL DDL `CREATE TABLE` provide already feed — see
    // `zzop_parser_typescript::adapters::entity_decorators` module doc.
    provides.extend(zzop_parser_typescript::extract_entity_db_table_provides(
        rel, text,
    ));
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
mod tests;
