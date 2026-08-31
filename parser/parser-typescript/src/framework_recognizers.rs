//! This crate's [`FRAMEWORK_RECOGNIZERS`] table — the capability declaration `zzop_core::recognizer`
//! defines and `zzop_engine::framework_recognizers` aggregates.
//!
//! It sits in its own file because it is the longest such table in the workspace and each row carries
//! the prose that keeps it honest; `lib.rs` was at the 300-line source cap with it inline. Note the
//! module is deliberately NOT under `src/adapters/` — that directory is this crate's recognizer root,
//! and a module there without a row of its own is a `rule_contracts::recognizer_drift` failure.

use zzop_core::recognizer::{channel, FrameworkRecognizer};

/// Frameworks this parser recognizes — see [`zzop_core::recognizer`] for what a declaration does and
/// does not claim.
///
/// This is the longest list in the workspace, and the reason is worth stating so it is not read as a
/// coverage target: roughly half of these have NO counterpart in another ecosystem (tRPC, Next.js
/// route files, Hono, Nest decorators are TypeScript-shaped). "Recognizer parity with TypeScript" is
/// therefore not a goal for any other parser — the goal is layer-2 population coverage per ecosystem
/// (`parser-expansion.md` §0), and the populations differ.
///
/// Several adapter MODULES are deliberately absent here because they are mechanisms rather than
/// frameworks — `class_shapes`, `wrapper_calls`, `global_prefix` and the `client_base` pair refine or
/// resolve what the framework rows above already found, and declaring them would answer "does zzop
/// know my stack" with our own module names. `pathname_dispatch` used to be listed in that sentence
/// and was moved OUT of it on 2026-08-01: it recognizes framework-less servers on its own evidence and
/// emits its own provides, so calling it a mechanism was simply wrong (see its row below).
pub const FRAMEWORK_RECOGNIZERS: &[FrameworkRecognizer] = &[
    FrameworkRecognizer {
        framework: "express",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::PROVIDES],
    },
    FrameworkRecognizer {
        framework: "nestjs",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::PROVIDES],
    },
    // Nest fills the auth-evidence channel twice over: `controller_decorators`' `@UseGuards` lines and
    // `nest_middleware`'s `forRoutes` patterns both feed the decorator-guard side channel that exempts
    // routes from `mutating-route-no-auth` — guard evidence, not io. Express/hono deliberately do NOT
    // carry this row: their guard words ride INSIDE the mount fragments and surface as `auth-guarded`
    // attributes on their own `io.provides` at compose time, not as a separate side channel.
    FrameworkRecognizer {
        framework: "nestjs",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::AUTH_EVIDENCE],
    },
    FrameworkRecognizer {
        framework: "next.js",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::PROVIDES],
    },
    // Framework-LESS servers that route by comparing `url.pathname` against string literals — raw
    // Cloudflare Workers, Node `http.createServer`, Deno/Bun `serve` (`pathname_dispatch`). The row is
    // spelled after the SHAPE rather than after a package because there is no package to name: the
    // honest claim is "a server that dispatches on `url.pathname` is recognized". Until 2026-08-01 this
    // module was carried as a `NOT_A_FRAMEWORK` exemption reading "route-shape heuristic shared by
    // several framework rows", which was false in both halves — no other row consumes it, and it mints
    // its own `io.provides` from its own per-function evidence gates.
    FrameworkRecognizer {
        framework: "pathname dispatch",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::PROVIDES],
    },
    // Hono fills BOTH sides of the join, and until 2026-08-01 this list said it filled one. The
    // provide side is `router_mounts`' `new Hono()` / `: Hono` receiver vocabulary, whose verb and
    // mount fragments the engine composes into `http` provides (`compose_router_mount_provides`); the
    // consume side is `hono_client`'s typed RPC calls. Worth naming what this was: `emits` exists
    // precisely so a parser cannot look whole while filling half a join, and this was the FIRST wrong
    // answer the field itself produced — the mechanism that catches an under-claiming PARSER does not
    // catch an under-claiming ROW, because nothing binds a row's channel set to the modules behind it.
    FrameworkRecognizer {
        framework: "hono",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::PROVIDES],
    },
    FrameworkRecognizer {
        framework: "hono",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::CONSUMES],
    },
    FrameworkRecognizer {
        framework: "trpc",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::PROVIDES],
    },
    FrameworkRecognizer {
        framework: "trpc",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::CONSUMES],
    },
    // The db kind splits by SIDE here, and the split is the whole disclosure on a `.ts` tree.
    // `typeorm` is the only one of these three that reads a table DECLARATION out of TypeScript
    // (`entity_decorators`' `@Entity`); the other two read QUERIES (`db_table_consume`'s Prisma
    // accessor chains, `raw_sql`'s SQL statement strings — whose own doc states that a `CREATE TABLE`
    // string is deliberately NOT a consume, and no adapter turns it into a provide either). Until
    // 2026-08-26 all three declared `io.provides:db-table`, so `ioChannels.zeroExtraction` on immich
    // — 77 `CREATE TABLE`s across 24 `.ts` files, 0 extracted — named `raw sql` as a recognizer this
    // build has for that channel, which reads as "the tree declares no tables" rather than "this
    // build cannot see them".
    FrameworkRecognizer {
        framework: "typeorm",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::DB_PROVIDES],
    },
    FrameworkRecognizer {
        framework: "typeorm",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::DB_CONSUMES],
    },
    FrameworkRecognizer {
        framework: "prisma client",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::DB_CONSUMES],
    },
    FrameworkRecognizer {
        framework: "raw sql",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::DB_CONSUMES],
    },
    FrameworkRecognizer {
        framework: "axios",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::CONSUMES],
    },
    FrameworkRecognizer {
        framework: "fetch",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::CONSUMES],
    },
    // `ky` and `$fetch` were MISSING from this list until 2026-08-01, which made the disclosure
    // understate what this build knows — the opposite drift direction from the one
    // `rule_contracts::recognizer_drift` catches, and invisible to it: that guard binds MODULES to
    // rows, and both of these live inside the already-declared `egress` module. The residual is
    // therefore known and stated rather than guessed at: a module's row set is guarded, the client
    // VOCABULARY inside one is not, so widening `egress/matchers.rs` needs a row added here by hand.
    FrameworkRecognizer {
        framework: "ky",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::CONSUMES],
    },
    FrameworkRecognizer {
        framework: "$fetch",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::CONSUMES],
    },
    // The rest of that same residual, paid down 2026-08-01: `egress/angular.rs` and
    // `egress/generated_client.rs` are two more client recognizers living inside the declared `egress`
    // module, and neither had a row. `angular` is the dependency-injected `HttpClient` idiom, hard-gated
    // on the file importing `@angular/common/http`; the generated row covers the three openapi codegen
    // families whose call sites carry the URL as a request-descriptor PROPERTY rather than an argument
    // (swagger-typescript-api's `.request({ path, method })`, openapi-typescript-codegen's
    // `__request(OpenAPI, { url, method })`, `@hey-api/openapi-ts`'s `.get({ url })`). Both tag their
    // consumes with their own `IoConsume::client` value (`"angular"`, `"generated"`), which is the same
    // vocabulary a reader of this list is asking about.
    FrameworkRecognizer {
        framework: "angular",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::CONSUMES],
    },
    FrameworkRecognizer {
        framework: "openapi generated client",
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"],
        emits: &[channel::CONSUMES],
    },
];
