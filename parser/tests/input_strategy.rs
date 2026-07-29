//! Shared arbitrary-source generator for the eight `parser/*/tests/no_panic_proptest.rs` property
//! tests, included with `#[path = "../../tests/input_strategy.rs"] mod input_strategy;`.
//!
//! Not eight copies, because eight copies of a corpus drift and then the eight tests stop testing the
//! same thing. Not a crate either: `crates/engine/build.rs` walks every crate under
//! `parser/` that has a `src/` to declare a `PARSER_FINGERPRINT`, and a cache-key version is not
//! something a test-only generator has any business owning. So it is a plain file that eight test
//! targets each compile into themselves — which is also why it sits under a `tests/` path rather than
//! next to the crates: `scripts/check-max-file-lines.sh` exempts `tests/`, on the stated policy that the
//! 300-line cap exists to keep SOURCE units small.
//!
//! ## Why these tests exist, given that panics are already caught
//!
//! `crates/engine/src/pipeline/parsers.rs` wraps every frontend in `std::panic::catch_unwind`, and the
//! workspace does not set `panic = "abort"`, so a parser panic genuinely cannot take the engine down.
//! That is exactly why a panic here is dangerous rather than loud: on `Err(_)` each of those arms
//! returns `(Vec::new(), ImportMap::new(), lexical_loc(text), broken = true, Vec::new())`. The file
//! keeps its line count and loses every symbol, every import, every io fact — silently demoted to the
//! lexical fallback, indistinguishable in the output from a file that legitimately declares nothing.
//! No finding is emitted, no diagnostic names the file, and the degraded result is then CACHED under
//! the file's content hash, so the loss persists across warm runs. A panicking parser is a silent
//! capability hole, not a crash — and a silent hole is what property testing is for.
//!
//! ## What is generated
//!
//! Pure random UTF-8 mostly bounces off a real parser's first token, so it is the minority ingredient.
//! The other two generators aim at depth:
//!
//! - `arbitrary_chars` (weight 1) — any `char`, control codes and non-BMP scalars included. This is the
//!   generator that reaches byte/char-boundary handling, `LineIndex`-style offset math, and the
//!   `u32`/`usize` span casts every frontend does.
//! - `token_soup` (weight 2) — a shuffled join of real keywords, brackets, quote openers and framework
//!   markers drawn from ALL eight languages at once. Grammatically wrong on purpose: it gets past the
//!   lexer, opens constructs it never closes, and lands in the recovery paths that hand-written
//!   malformed fixtures never think to write.
//! - `mutated_seed` (weight 2) — a realistic snippet with 1..=4 random edits (truncate / delete a span /
//!   splice in a token / duplicate a span). This is the one that reaches the adapters: an extractor that
//!   only runs after `@Controller` or `CREATE TABLE` has been recognized is unreachable from noise.
//!
//! Every seed is fed to every frontend, not only its own language's. That is the real production case
//! (a `.ts` extension over a file that is actually SQL) and it is free here.
//!
//! ## Case budget
//!
//! Each crate's `CASES` is chosen so that its property costs roughly the same WALL TIME as the others
//! (~0.4-1.2 s each, ~6 s for all eight), not so that all eight run the same number of cases. The
//! per-case cost differs by an order of magnitude across frontends — measured 2026-07-29 on a warm
//! debug build, one case is ~0.12 ms for the regex scanners (sql, prisma), ~1.1 ms for swc and ruff
//! (typescript, python), and ~7-18 ms for the tree-sitter frontends (go, java, csharp, which re-parse
//! once per entry point) — so an equal case count would have meant one crate paying for all the others.
//!
//! A budget rather than a maximum, because a gate that slows `cargo test --workspace` noticeably is a
//! gate that eventually gets switched off, and this suite's value is in running on every commit forever.
//! Depth comes from repetition instead: proptest seeds from entropy on each run, so consecutive CI runs
//! explore different inputs, and `PROPTEST_CASES=20000` turns any of them into a deep run on demand
//! (see [`config`]).
//!
//! ## Out of scope
//!
//! Stack exhaustion from deeply nested input. A recursive-descent frontend given 100k open parens
//! aborts the process rather than unwinding, so `catch_unwind` does not help and neither does proptest
//! — the failure has no reproducer to shrink and would make the gate flaky. Generated inputs are capped
//! (`MAX_CHARS`) well below that regime; nesting depth is a separate question with a separate answer.

#![allow(dead_code)]

use proptest::prelude::*;

/// Upper bound on generated length, in `char`s. `mutated_seed`'s duplicate op can grow a seed on every
/// edit, so the cap is what keeps a pathological case from turning one test case into a full second of
/// swc/tree-sitter/syn work. Large enough that multi-declaration files and long token runs still occur.
const MAX_CHARS: usize = 8_000;

/// Declaration keywords, all eight languages in one pool.
const KEYWORDS: &[&str] = &[
    "class",
    "function",
    "fn",
    "def",
    "func",
    "impl",
    "struct",
    "enum",
    "interface",
    "namespace",
    "trait",
    "record",
    "module",
    "package",
    "type",
    "const",
    "let",
    "var",
    "static",
    "public",
    "private",
    "protected",
    "internal",
    "abstract",
    "override",
    "virtual",
    "partial",
    "readonly",
    "declare",
    "export",
    "default",
    "import",
    "from",
    "using",
    "require",
    "pub",
    "mod",
    "async",
    "await",
    "return",
    "yield",
    "if",
    "else",
    "for",
    "while",
    "match",
    "switch",
    "case",
    "try",
    "catch",
    "finally",
    "throw",
    "raise",
    "with",
    "as",
    "in",
    "of",
    "new",
    "this",
    "self",
    "super",
    "extends",
    "implements",
    "lambda",
    "unsafe",
    "dyn",
    "where",
    "let mut",
];

/// Punctuation and operators, including the ones that open something the soup will not close, plus the
/// comment and string openers with no closer — the classic unterminated-token recovery paths — and the
/// numeric literal shapes that are lexically incomplete.
const PUNCTUATION: &[&str] = &[
    "{", "}", "(", ")", "[", "]", "<", ">", ";", ":", ",", ".", "=>", "->", "::", "=", "==", "===",
    "+", "-", "*", "/", "%", "&", "|", "^", "!", "?", "??", "?.", "#", "@", "$", "\\", "...", "..",
    "//", "/*", "*/", "/**", "--", "#!", "\"", "'", "`", "\"\"\"", "'''", "r#\"", "b\"", "$\"",
    "\"a\"", "'b'", "`t${x}`", "${", "0", "0x", "0b", "1e", "1_000", ".5", "u8", "f64", "L", "UL",
];

/// Framework and DSL markers the adapters gate on: without these in the pool, every extractor that runs
/// only after recognizing one of them is unreachable from generated input.
const MARKERS: &[&str] = &[
    "@Controller",
    "@Get",
    "@Post",
    "@Entity",
    "@Injectable",
    "@Module",
    "@RequestMapping",
    "@GetMapping",
    "@PreAuthorize",
    "@app.get",
    "@router.post",
    "@login_required",
    "[HttpGet]",
    "[ApiController]",
    "[Authorize]",
    "#[derive(Debug)]",
    "#[tokio::main]",
    "prisma.",
    "$queryRaw",
    "axios.get",
    "fetch(",
    "router.get",
    "app.use",
    "http.HandleFunc",
    "gorm.Model",
    "db.Query",
    "SELECT",
    "INSERT INTO",
    "CREATE TABLE",
    "FROM",
    "WHERE",
    "JOIN",
    "model",
    "datasource",
    "generator",
    "@@map",
    "@id",
    "@relation",
    "<template>",
    "<script>",
    "</script>",
    "<?xml",
];

/// Whitespace and format controls that are still text — NUL, BOM, RTL override, no-break space.
const CONTROLS: &[&str] = &["\u{0}", "\u{feff}", "\u{202e}", "\u{a0}", "\r\n", "\t"];

/// The four pools above as one list — the token vocabulary `token_soup` draws from and `mutated_seed`
/// splices in. Split into four consts only because an array literal with interior comments is one
/// element per line under rustfmt, which would make this file five times its length for no reader.
fn tokens() -> Vec<&'static str> {
    KEYWORDS
        .iter()
        .chain(PUNCTUATION)
        .chain(MARKERS)
        .chain(CONTROLS)
        .copied()
        .collect()
}

/// Separators between soup tokens. The empty one matters: it welds tokens into single identifiers.
const SEPARATORS: &[&str] = &["", " ", "\n", "\t", "\r\n", "  ", "\n\n"];

/// Realistic snippets, one per frontend. Short by design — `mutated_seed` is what makes them
/// interesting, and a long seed only dilutes the mutation rate.
const SEEDS: &[&str] = &[
    // TypeScript / NestJS
    "import { Controller, Get } from '@nestjs/common';\nimport axios from 'axios';\n\n@Controller('users')\nexport class UsersController {\n  @Get(':id')\n  async find(id: string) {\n    return axios.get(`/api/users/${id}`);\n  }\n}\n",
    // Python / FastAPI + SQLAlchemy
    "from fastapi import APIRouter, Depends\nimport httpx\n\nrouter = APIRouter(prefix=\"/orders\")\n\n@router.get(\"/{oid}\")\nasync def get_order(oid: int, user=Depends(require_user)):\n    async with httpx.AsyncClient() as c:\n        return (await c.get(f\"/api/orders/{oid}\")).json()\n",
    // Java / Spring
    "package com.example.api;\n\nimport org.springframework.web.bind.annotation.*;\n\n@RestController\n@RequestMapping(\"/v1/items\")\npublic class ItemController {\n  @GetMapping(\"/{id}\")\n  @PreAuthorize(\"hasRole('USER')\")\n  public Item get(@PathVariable Long id) { return repo.findById(id); }\n}\n",
    // Go / net-http + gorm
    "package main\n\nimport (\n\t\"net/http\"\n\n\t\"gorm.io/gorm\"\n)\n\ntype User struct{ gorm.Model }\n\nfunc main() {\n\thttp.HandleFunc(\"/api/users\", func(w http.ResponseWriter, r *http.Request) {\n\t\tdb.Table(\"users\").Find(&[]User{})\n\t})\n}\n",
    // Rust / axum
    "use axum::{routing::get, Router};\n\n#[derive(Debug)]\npub struct AppState;\n\npub async fn handler(State(s): State<AppState>) -> &'static str { \"ok\" }\n\npub fn app() -> Router {\n    Router::new().route(\"/api/health\", get(handler))\n}\n",
    // C# / ASP.NET
    "using System.Net.Http;\nusing Microsoft.AspNetCore.Mvc;\n\nnamespace Api.Controllers;\n\n[ApiController]\n[Route(\"api/[controller]\")]\npublic class OrdersController : ControllerBase\n{\n    [HttpGet(\"{id}\")]\n    [Authorize]\n    public IActionResult Get(int id) => Ok(id);\n}\n",
    // Prisma schema
    "datasource db {\n  provider = \"postgresql\"\n  url      = env(\"DATABASE_URL\")\n}\n\nenum Role {\n  USER\n  ADMIN\n}\n\nmodel User {\n  id    Int    @id @default(autoincrement())\n  email String @unique\n  role  Role\n  @@map(\"users\")\n}\n",
    // SQL DDL + DML
    "CREATE TABLE IF NOT EXISTS public.\"orders\" (\n  id BIGSERIAL PRIMARY KEY,\n  user_id BIGINT REFERENCES users(id)\n);\n\nINSERT INTO orders (user_id) VALUES (1);\nSELECT o.id FROM orders o JOIN users u ON u.id = o.user_id WHERE u.email = $1;\n",
];

/// Any `char` sequence — control codes, format controls and non-BMP scalars included.
fn arbitrary_chars() -> impl Strategy<Value = String> {
    proptest::collection::vec(any::<char>(), 0..512).prop_map(|cs| cs.into_iter().collect())
}

/// Grammatically wrong but lexically real: keywords, brackets and markers joined by random whitespace.
fn token_soup() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        (
            proptest::sample::select(tokens()),
            proptest::sample::select(SEPARATORS),
        ),
        0..128,
    )
    .prop_map(|parts| {
        let mut out = String::new();
        for (tok, sep) in parts {
            out.push_str(tok);
            out.push_str(sep);
        }
        out
    })
}

/// A realistic snippet with 1..=4 random edits applied at `char` granularity.
fn mutated_seed() -> impl Strategy<Value = String> {
    (
        proptest::sample::select(SEEDS),
        proptest::collection::vec(
            (
                any::<usize>(),
                any::<usize>(),
                proptest::sample::select(tokens()),
                0u8..4,
            ),
            1..5,
        ),
    )
        .prop_map(|(seed, edits)| {
            let mut chars: Vec<char> = seed.chars().collect();
            for (a, b, tok, op) in edits {
                if chars.is_empty() {
                    chars.extend(tok.chars());
                    continue;
                }
                let i = a % chars.len();
                let j = b % chars.len();
                let (lo, hi) = if i <= j { (i, j) } else { (j, i) };
                match op {
                    0 => chars.truncate(lo),
                    1 => {
                        chars.drain(lo..hi);
                    }
                    2 => {
                        let ins: Vec<char> = tok.chars().collect();
                        chars.splice(lo..lo, ins);
                    }
                    _ => {
                        let dup: Vec<char> = chars[lo..hi].to_vec();
                        chars.splice(hi..hi, dup);
                    }
                }
                chars.truncate(MAX_CHARS);
            }
            chars.into_iter().collect()
        })
}

/// The generator every `no_panic_proptest.rs` draws from. Weights favour the two structured generators
/// (4:1 over pure noise) because depth, not volume, is what finds a parser bug.
pub fn source_text() -> impl Strategy<Value = String> {
    prop_oneof![
        1 => arbitrary_chars(),
        2 => token_soup(),
        2 => mutated_seed(),
    ]
}

/// `ProptestConfig` with a per-crate default case count that `PROPTEST_CASES` can still override.
///
/// Written out rather than using `ProptestConfig { cases, ..default() }` because that form pins the
/// count in the binary: proptest reads `PROPTEST_CASES` inside `default()`, and a struct-update literal
/// then overwrites whatever the environment said. A gate whose depth cannot be turned up from CI
/// without editing eight files is a gate nobody ever turns up.
///
/// Failure persistence is left at proptest's default. On the first failure it prints
/// "FileFailurePersistence::SourceParallel set, but failed to find lib.rs or main.rs" and falls back to
/// writing `parser/<crate>/tests/no_panic_proptest.proptest-regressions` — the message is about an
/// integration test having no `src/` to sit beside, not about persistence being broken, and the fallback
/// path is the one we want. COMMIT that file if it ever appears: it is the reproducing input, replayed
/// ahead of the random cases on every later run, which is what turns a found panic into a regression
/// test instead of a story about one unlucky seed.
pub fn config(cases: u32) -> ProptestConfig {
    let mut config = ProptestConfig::default();
    if std::env::var_os("PROPTEST_CASES").is_none() {
        config.cases = cases;
    }
    config
}

/// Fixed inputs every frontend is hammered with in addition to the generated ones — the deterministic
/// floor, so the suite still asserts something when the RNG seed changes. Each one is a shape a
/// generator can produce but is not guaranteed to on any given run.
pub const FIXED_EDGE_CASES: &[&str] = &[
    "",
    "\u{0}",
    "\u{feff}",
    "\r",
    "\n\n\n",
    "\u{feff}\u{0}\u{202e}",
    "\"",
    "'''",
    "/*",
    "${",
    "{{{{{{{{",
    "))))))))",
    "\\",
    "\u{10ffff}",
    "a\u{0}b\u{feff}c",
    "SELECT * FROM \"",
    "@Controller(",
    "model User {",
    "#[derive(",
    "<script>",
];
