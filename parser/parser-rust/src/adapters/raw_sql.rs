//! Raw-SQL-string -> `db-table` CONSUME extraction — the Rust member of the ORM-LESS db-table consume
//! family, and the FIRST db-channel producer this crate has had at all. Before it, `parser-rust`
//! declared two recognizers (`axum` provides, `reqwest` consumes) and **zero** db ones: every `db`/`sql`
//! rule and the whole `db-table` half of the cross-layer join were structurally silent on `.rs`, so a
//! Rust service whose schema is declared by a `migrations/*.sql` file (already minting `table:*` provides
//! through `zzop_parser_sql`) showed that schema as "declared, consumed by nobody". Same defect shape the
//! TypeScript sibling `zzop_parser_typescript::adapters::raw_sql` was built for, one language over.
//!
//! ## Why a SHAPE recognizer and not a per-crate one
//! Rust's database crates disagree about everything except one thing: the SQL is a string in the source.
//! `sqlx::query!("SELECT ... FROM users")`, `tokio_postgres`'s `client.query("...", &[])`, `rusqlite`'s
//! `conn.execute("...", [])`, `diesel::sql_query("...")`, `sea_orm::Statement::from_string(backend,
//! "...")` — one string-shaped recognizer covers all of them, where five crate-gated recognizers would
//! cover exactly the five and go silent on the sixth. The precision gate is therefore the SQL statement
//! SHAPE (an UPPERCASE statement head — see [`zzop_parser_sql::extract_statement_table_refs`]'s doc for
//! why letter case is the load-bearing discriminator against English prose), never the call site.
//!
//! ## Where the SQL vocabulary lives
//! Nowhere in this file. This adapter contributes exactly one thing — the set of string values a `.rs`
//! file contains, with their source lines — and hands each to `zzop_parser_sql`, which owns the
//! statement-shape gate, the table extraction, and the channel's canonical casing. That is the same
//! split the DDL provide side uses, so a CONSUME key and the PROVIDE key for one physical table cannot
//! drift.
//!
//! ## Macro bodies are walked, because sqlx lives inside one
//! `syn` hands a macro invocation's arguments back as an opaque `proc_macro2::TokenStream` and
//! `syn::visit` does NOT descend into it (the crate-root "Scope note: macros" says so). Relying on the
//! default walk would therefore have missed `sqlx::query!`/`query_as!`/`query_scalar!` — sqlx's DOMINANT
//! idiom, and the single most likely raw-SQL shape in a Rust tree — while looking like coverage. So
//! [`RawSqlCollector::visit_macro`] walks the token stream itself and reads every string literal token
//! out of it. Nothing is interpreted: a token is either a string literal (read) or it is not (ignored).
//!
//! ## Interpolation is never guessed
//! Every Rust format placeholder (`{}`, `{name}`, `{0:>8}`) is replaced by an identifier-SHAPED sentinel
//! before the SQL scan, so it fuses with adjacent identifier characters instead of letting them stand
//! alone; a table name that ends up containing the sentinel is dropped. `format!("SELECT * FROM users
//! WHERE id = {}", id)` yields `users` (the table is literal), while `format!("SELECT * FROM {}", t)` and
//! `format!("... FROM {a}_{b}")` yield nothing rather than a fabricated `table:users_` — the same
//! never-guess boundary `http_clients`' URL resolution draws. A `{` that does not open a placeholder
//! (unbalanced, or wrapping quoted/whitespaced text such as a JSON literal `'{"a":1}'`) is left alone,
//! so it can never swallow the `FROM` that follows it.
//!
//! ## Deliberately NOT recognized (stated, because a silent skip reads as coverage)
//! - **`sqlx::query_file!("queries/x.sql")`** — the argument is a PATH, not SQL. It fails the statement
//!   gate and contributes nothing; resolving the referenced file is out of this adapter's scope.
//! - **Statement text assembled by `+`/`push_str`, or held in another file's `const`** — the string this
//!   file sees must itself be statement-shaped. sqlx's `QueryBuilder` is this case.
//! - **Byte strings** (`b"SELECT ..."`) — not a `LitStr`, so never a candidate.
//! - **Lowercase-keyword SQL** — inherited from the shared statement gate, not decided here.
//! - **`CREATE TABLE`** — DDL is the provide side's business (`zzop_parser_sql::extract`).
//!
//! ## Test surface is excluded, and Rust needs THREE gates for that
//! The TypeScript sibling gates on the file path alone, which is enough there. Rust's unit tests live
//! INSIDE the shipping file (`#[cfg(test)] mod tests`), so a path gate alone would count a fixture's
//! `"SELECT ... FROM users"` as deployed DB coupling. Applied here, in this order:
//! 1. `zzop_core::is_test_file` on the PATH — the whole-file axis the other parsers share.
//! 2. The file's own INNER attributes — a `#![cfg(test)]` at the top of the file gates everything under
//!    it without any item below carrying an attribute of its own, so nothing else would catch it.
//! 3. A subtree SKIP on every test-gated `Item` (all variants — `mod`, `fn`, `impl`, and `const`/`static`
//!    too), `ImplItem` and `TraitItem`.
//!
//! Nothing else is gated, and that residual is real: a string reached only through a path this walk does
//! not model still contributes. What is NO LONGER residual is the node axis — (3) covers exactly the
//! three node kinds `lang::test_spans` walks, which is what makes "this line is inside a test span" and
//! "this fact was suppressed" the same answer instead of two.
//!
//! The predicate behind gates (2) and (3) was FIRST written here and no longer lives here: it is
//! [`crate::lang::test_spans::is_test_gated`], because the rule packs needed the same question answered
//! for every `.rs` line and two copies of "what makes an item test-only" would have drifted. This module
//! still SKIPS rather than recording a span — never minting a `db-table` consume is cheaper than minting
//! one and subtracting it, and it keeps the channel clean for every consumer, not just rule packs.

use proc_macro2::{TokenStream, TokenTree};
use syn::visit::{self, Visit};
use syn::{Attribute, ImplItem, Item, LitStr, Macro, TraitItem};
use zzop_core::IoConsume;

use crate::lang::test_spans::{
    impl_item_is_test_gated, is_test_gated, item_is_test_gated, trait_item_is_test_gated,
};

/// Stands in for a format placeholder. Identifier-shaped on purpose: it must GLUE to identifier
/// characters next to it (so `{a}_{b}` cannot leave a bare `_` behind that reads as a table), and it
/// survives `zzop_core::db_table_channel_casing` (which lower-cases only the FIRST character) unchanged
/// from the second character on, so the veto below can find it by substring. Deliberately the same
/// spelling `zzop_parser_typescript::adapters::raw_sql` uses — one sentinel, two languages, so a reader
/// comparing the two adapters is not comparing two conventions. No real table can be named this.
const PLACEHOLDER: &str = "_zzopTplHole_";

/// The longest a format placeholder's body may be before it is treated as ordinary text. Real specs are
/// short (`{}`, `{name}`, `{0:>8.3}`); a long run to the next `}` is far likelier to be prose or an
/// embedded JSON/JSONB literal, and consuming it would delete the `FROM` behind it.
const MAX_PLACEHOLDER_BODY: usize = 64;

/// Extract `db-table` CONSUME entries from the SQL statement strings one `.rs` file contains. Keyed
/// `table:<name>` at extraction time (the join channel's canonical casing), anchored at the line the
/// string literal starts on. Empty for a test file and for an unparseable one — the same conventions the
/// sibling adapters in this crate apply.
pub fn extract_rust_raw_sql_db_table_consumes(rel: &str, text: &str) -> Vec<IoConsume> {
    // A test fixture's SQL is not deployed DB coupling — skip before parsing, mirroring the sibling
    // db-table consume adapters in every other parser crate.
    if zzop_core::is_test_file(rel) {
        return Vec::new();
    }
    let Some(file) = crate::parse_file(text) else {
        return Vec::new();
    };
    // `#![cfg(test)]` — an INNER attribute gating the whole file. Nothing below it carries an attribute
    // of its own, so the subtree skips below would find nothing to skip and every fixture string in the
    // file would mint a deployed `db-table` consume. Same check `lang::test_spans` makes first.
    if is_test_gated(&file.attrs) {
        return Vec::new();
    }
    let mut collector = RawSqlCollector {
        file: rel,
        out: Vec::new(),
    };
    collector.visit_file(&file);
    collector.out
}

struct RawSqlCollector<'a> {
    file: &'a str,
    out: Vec<IoConsume>,
}

impl RawSqlCollector<'_> {
    /// Runs one candidate string through the shared SQL extractor and records a consume per distinct
    /// table it names.
    fn collect(&mut self, raw: &str, line: u32) {
        let masked = mask_format_holes(raw);
        for table in zzop_parser_sql::extract_statement_table_refs(&masked) {
            if table.contains(PLACEHOLDER) {
                continue; // a name built out of an interpolation — never guessed
            }
            self.out.push(IoConsume {
                kind: "db-table".into(),
                key: Some(format!("table:{table}")),
                file: self.file.into(),
                line,
                raw: None,
                method: None,
                body: None,
                client: None,
                retry_configured: None,
            });
        }
    }

    /// Reads every string-literal token out of a macro's opaque argument stream, groups included — see
    /// the module doc's "Macro bodies are walked" section. Recursion depth is the token-group nesting
    /// depth, which `proc_macro2`'s own lexer already descended to build this stream, so this walk
    /// cannot reach a depth the parse itself survived.
    fn walk_macro_tokens(&mut self, tokens: TokenStream) {
        for tt in tokens {
            match tt {
                TokenTree::Group(g) => self.walk_macro_tokens(g.stream()),
                TokenTree::Literal(l) => {
                    // The literal's OWN span carries the line; `parse2` is used (rather than
                    // `syn::Lit::new`, which panics on an unrecognized literal) so a non-string token
                    // degrades to a skip. Every parser here upholds a no-panic contract on arbitrary
                    // input — see this crate's `tests/no_panic_proptest.rs`.
                    let line = l.span().start().line as u32;
                    if let Ok(s) = syn::parse2::<LitStr>(TokenStream::from(TokenTree::Literal(l))) {
                        self.collect(&s.value(), line);
                    }
                }
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for RawSqlCollector<'_> {
    fn visit_lit_str(&mut self, s: &'ast LitStr) {
        self.collect(&s.value(), crate::line_of(s));
    }

    /// A `///` doc comment is an `#[doc = "..."]` attribute in the AST, so prose ABOUT a query
    /// (`/// Runs SELECT id FROM users.`) would otherwise mint a real `db-table` consume from a comment.
    /// swc gives the TypeScript sibling no such literal, which is why only this crate needs the skip.
    fn visit_attribute(&mut self, a: &'ast Attribute) {
        if a.path().is_ident("doc") {
            return;
        }
        visit::visit_attribute(self, a);
    }

    /// Every `Item` variant, not the three (`mod`/`fn`/`impl`) this once named: a `#[cfg(test)] const
    /// FIXTURE_SQL: &str = "SELECT ..."` is an `Item::Const` and was extracted as deployed coupling for
    /// exactly that reason. The three axes below are `lang::test_spans`' own, so this gate can no longer
    /// be narrower than the span the rule packs subtract with.
    fn visit_item(&mut self, i: &'ast Item) {
        if item_is_test_gated(i) {
            return;
        }
        visit::visit_item(self, i);
    }

    fn visit_impl_item(&mut self, i: &'ast ImplItem) {
        if impl_item_is_test_gated(i) {
            return;
        }
        visit::visit_impl_item(self, i);
    }

    fn visit_trait_item(&mut self, i: &'ast TraitItem) {
        if trait_item_is_test_gated(i) {
            return;
        }
        visit::visit_trait_item(self, i);
    }

    fn visit_macro(&mut self, mac: &'ast Macro) {
        visit::visit_macro(self, mac);
        self.walk_macro_tokens(mac.tokens.clone());
    }
}

/// Replaces each Rust format placeholder with [`PLACEHOLDER`], leaving `{{`/`}}` escapes and any `{`
/// that does not open a placeholder untouched — see the module doc's "Interpolation is never guessed".
fn mask_format_holes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('{') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        if let Some(stripped) = after.strip_prefix('{') {
            out.push_str("{{");
            rest = stripped;
            continue;
        }
        match placeholder_body_len(after) {
            Some(len) => {
                out.push_str(PLACEHOLDER);
                rest = &after[len + 1..]; // +1 skips the closing `}`
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Byte length of the placeholder BODY (the opening `{` already consumed) when `after` opens a Rust
/// format placeholder, else `None`. A body carrying whitespace, a quote, or a nested `{` is not a format
/// spec — it is prose or an embedded literal — and an unterminated one is not a placeholder at all.
fn placeholder_body_len(after: &str) -> Option<usize> {
    let end = after.find('}')?;
    if end > MAX_PLACEHOLDER_BODY {
        return None;
    }
    let body = &after[..end];
    if body
        .chars()
        .any(|c| c.is_whitespace() || c == '"' || c == '\'' || c == '{')
    {
        return None;
    }
    Some(end)
}

#[cfg(test)]
mod tests;
