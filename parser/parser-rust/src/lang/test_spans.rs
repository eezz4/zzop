//! Per-file TEST-ONLY line spans — the `.rs` answer to "is this line shipped code, or is it a fixture?"
//!
//! ## Why a path pattern could never answer this
//! Every other language this workspace parses puts its tests in a FILE the path names: `foo.test.ts`,
//! `tests/test_foo.py`, `src/test/java/...`. That is why the shared DSL fragment `${test-paths-stories}`
//! is a directory/filename regex, and why it is enough there. Rust's DOMINANT convention is the exact
//! opposite — `#[cfg(test)] mod tests { ... }` sits INSIDE the shipping file, compiled out of the release
//! binary by an attribute, not by a path. No path-shaped exclusion can see it, and widening the shared
//! fragment with `tests?\.rs$` is a partial fix that moves the number without touching the dominant case.
//!
//! Measured on this repo (2026-08-02, four packs temporarily enabled): 98 findings on `.rs` files, of
//! which **zero** were in shipped code — every one sat under `#[cfg(test)]` or `#[test]`. So the honest
//! reading of that run was never "the Rust rules are noisy"; it was "there is no axis that separates Rust
//! test code from Rust shipped code", and this module is that axis.
//!
//! ## What a span is
//! One entry per test-gated ITEM: `(first line, last line)` inclusive, 1-based, covering the item's own
//! attributes (a `#[test]` line is part of the test). The whole subtree is covered by the one span — a
//! `mod tests` containing forty functions yields ONE span, not forty, because the visitor stops
//! descending once an enclosing item is already gated. Spans never overlap for that reason, but a
//! consumer must not rely on it: the contract is only "a line inside any span is test-only".
//!
//! An inner `#![cfg(test)]` at the top of a file gates the whole file, and is emitted as a single
//! `(1, line_count)` span rather than by walking anything.
//!
//! ## What it deliberately does NOT see
//! A file whose test-ness is declared by its PARENT (`#[cfg(test)] mod helpers;` pointing at
//! `helpers.rs`) carries no attribute of its own, so nothing in this file's text proves it. That is a
//! cross-file fact and this is a per-file projection; the path axis (`${test-paths-stories}`,
//! `zzop_core::is_test_file`) remains the owner of "the whole file is a test file". The two axes are
//! complements, not substitutes.
//!
//! ## One owner for "is this item test-gated"
//! [`is_test_gated`] is that owner. `adapters::raw_sql` had the FIRST copy of this predicate — it needed
//! it because a fixture's `"SELECT ... FROM users"` inside `#[cfg(test)] mod tests` was otherwise
//! extracted as deployed DB coupling — and it now calls in here rather than keeping its own, as does
//! every other adapter in this crate. The two USES stay different on purpose: an adapter SKIPS the
//! subtree (never extracting a fact is cheaper and keeps the channel clean at the source), while this
//! module RECORDS it (a rule pack needs the region to still exist so it can be subtracted from
//! findings). Same question, two answers, one predicate.
//!
//! Sharing the predicate is not enough on its own — a consumer that asks it about fewer nodes than IT
//! ITSELF READS is strictly narrower than the span it is supposed to agree with, and the two then answer
//! the same file oppositely. So the node axis is shared too: [`item_is_test_gated`],
//! [`impl_item_is_test_gated`] and [`trait_item_is_test_gated`] are the three questions the visitor below
//! asks, and an adapter asks whichever of them its own walk can reach, plus the file-level
//! `#![cfg(test)]` check [`extract_test_spans`] makes first.
//!
//! "Whichever it can reach", not "all three", because the bar is COVERAGE OF THE ADAPTER'S OWN REACH and
//! not symmetry with this visitor. `adapters::raw_sql` and `adapters::http_clients` are `Visit` walks
//! that descend the whole file, so all three apply to them; `adapters::axum` reads `syn::File::items`
//! and scans only `Item::Fn`, never entering an `impl`, a `trait` or a nested `mod`, so
//! [`item_is_test_gated`] on that loop already covers everything it could emit from — the other two
//! would be unreachable code, and its module doc pins the measurement that says so.

use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Attribute, ImplItem, Item, Meta, MetaList, Token, TraitItem};

/// Extract this file's test-only line spans — see module doc. Empty for an unparseable file (the same
/// degrade-to-nothing contract every `extract_*` in this crate upholds), which is the SAFE direction
/// here: no span means nothing is subtracted, so a parse failure can never silence a rule.
pub fn extract_test_spans(_rel: &str, text: &str) -> Vec<(u32, u32)> {
    let Some(file) = crate::parse_file(text) else {
        return Vec::new();
    };
    // `#![cfg(test)]` — an INNER attribute gating the whole file. Walking would find nothing (the items
    // below it carry no attribute of their own), so the file's own line count is the span.
    if is_test_gated(&file.attrs) {
        return vec![(1, crate::count_loc(text))];
    }
    let mut collector = TestSpanCollector { out: Vec::new() };
    collector.visit_file(&file);
    collector.out
}

struct TestSpanCollector {
    out: Vec<(u32, u32)>,
}

impl TestSpanCollector {
    /// Records `node`'s span when `attrs` gate it as test-only, and answers whether the caller should
    /// STOP descending. Stopping is what keeps one `mod tests` from becoming one span per item inside it.
    fn gated<T: Spanned>(&mut self, attrs: &[Attribute], node: &T) -> bool {
        if !is_test_gated(attrs) {
            return false;
        }
        let span = node.span();
        self.out
            .push((span.start().line as u32, span.end().line as u32));
        true
    }
}

impl<'ast> Visit<'ast> for TestSpanCollector {
    fn visit_item(&mut self, i: &'ast Item) {
        if self.gated(item_attrs(i), i) {
            return;
        }
        visit::visit_item(self, i);
    }

    /// `#[test] fn` inside an `impl` block, and `#[cfg(test)] const FIXTURE` beside it. Reached only when
    /// the enclosing `impl` was NOT itself gated (that case already returned above).
    fn visit_impl_item(&mut self, i: &'ast ImplItem) {
        if self.gated(impl_item_attrs(i), i) {
            return;
        }
        visit::visit_impl_item(self, i);
    }

    /// The trait-definition twin of `visit_impl_item` — a default method body can be `#[cfg(test)]` too.
    fn visit_trait_item(&mut self, i: &'ast TraitItem) {
        if self.gated(trait_item_attrs(i), i) {
            return;
        }
        visit::visit_trait_item(self, i);
    }
}

/// True when these attributes make the annotated item TEST-ONLY:
/// - any attribute whose LAST path segment is `test` — covers `#[test]`, `#[tokio::test]`,
///   `#[sqlx::test]`, `#[actix_web::test]` and every other runner without enumerating one;
/// - `#[cfg(<predicate>)]` whose predicate cannot hold outside a test build — see [`implies_test`].
///
/// The one owner of this question in this crate — see the module doc's "One owner" section for why
/// `adapters::raw_sql` calls in here instead of keeping the copy it originally wrote.
pub(crate) fn is_test_gated(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        if a.path().segments.last().is_some_and(|s| s.ident == "test") {
            return true;
        }
        match &a.meta {
            // `cfg` takes exactly ONE predicate, so the list below is a one-element list in every
            // well-formed case; `all` is required (rather than `any`) so a malformed multi-predicate
            // `cfg` degrades toward NOT test-only, which is the direction that never deletes a fact.
            Meta::List(l) if a.path().is_ident("cfg") => predicates(l)
                .is_some_and(|preds| !preds.is_empty() && preds.iter().all(implies_test)),
            _ => false,
        }
    })
}

/// Does this `cfg` predicate hold ONLY when `test` is enabled? Judged on the predicate's STRUCTURE, not
/// on whether the ident `test` appears somewhere in its tokens — a flat ident search cannot tell
/// `all(test, not(miri))` (test-only) from `not(test)` (shipping) from `any(test, feature = "kit")`
/// (shipped whenever the feature is on), and answering all three the same way is how a fixture's SQL got
/// minted as deployed DB coupling. One rule per connective:
/// - **`test`** — the base case, and the only predicate that implies itself.
/// - **`all(P…)`** — a conjunction holds only when every `P` does, so ONE test-implying conjunct is
///   enough: `all(test, not(miri))` and `all(test, feature = "x")` are test-only.
/// - **`any(P…)`** — a disjunction can hold because of any single `P`, so it is test-only only when
///   EVERY branch is: `any(test, feature = "kit")` ships whenever `kit` is on, and gating it would
///   delete real findings from code a user compiles.
/// - **`not(P)`** — never test-only here. `not(test)` is the exact inverse (it gates code compiled OUT
///   of the test build and INTO the release binary), and for any other `P` the negation says nothing
///   about `test`. Spelled out because the negation is the one case whose obvious reading is backwards.
/// - **anything else** (`feature = "x"`, `target_os = "…"`, a bare ident like `miri`, a connective this
///   crate does not model) — not test-implying. Unknown means shipping: silence is the safe answer for
///   the fact channels, and merely noisy for the rule packs.
fn implies_test(meta: &Meta) -> bool {
    match meta {
        Meta::Path(p) => p.is_ident("test"),
        Meta::List(l) => match predicates(l) {
            Some(inner) if l.path.is_ident("all") => inner.iter().any(implies_test),
            Some(inner) if l.path.is_ident("any") => {
                !inner.is_empty() && inner.iter().all(implies_test)
            }
            _ => false,
        },
        Meta::NameValue(_) => false,
    }
}

/// The comma-separated predicate list inside `all(...)`/`any(...)`/`cfg(...)`. `None` when the tokens are
/// not a `Meta` list at all (a `cfg` spelling `syn` cannot model), which every caller reads as "not
/// test-implying" rather than guessing.
fn predicates(list: &MetaList) -> Option<Punctuated<Meta, Token![,]>> {
    list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .ok()
}

/// Is this `Item` (any variant) test-gated? The [`is_test_gated`] entry point a VISITOR needs — every
/// adapter in this crate skips a subtree on it, and each must ask about every node axis ITS OWN walk
/// reaches, or its gate would be strictly narrower than the span this module records.
pub(crate) fn item_is_test_gated(i: &Item) -> bool {
    is_test_gated(item_attrs(i))
}

/// [`item_is_test_gated`]'s `ImplItem` twin — `impl Repo { #[cfg(test)] fn seed(&self) { … } }`.
pub(crate) fn impl_item_is_test_gated(i: &ImplItem) -> bool {
    is_test_gated(impl_item_attrs(i))
}

/// [`item_is_test_gated`]'s `TraitItem` twin — a `#[cfg(test)]` default method body in a trait.
pub(crate) fn trait_item_is_test_gated(i: &TraitItem) -> bool {
    is_test_gated(trait_item_attrs(i))
}

/// `syn::Item` carries `attrs` on every variant but exposes no accessor for them, so the match is
/// written out once here rather than at each visitor. `Item` is `#[non_exhaustive]`, hence the arm:
/// an item shape a future `syn` adds contributes no span rather than failing to compile, and the walk
/// still descends into it.
fn item_attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(i) => &i.attrs,
        Item::Enum(i) => &i.attrs,
        Item::ExternCrate(i) => &i.attrs,
        Item::Fn(i) => &i.attrs,
        Item::ForeignMod(i) => &i.attrs,
        Item::Impl(i) => &i.attrs,
        Item::Macro(i) => &i.attrs,
        Item::Mod(i) => &i.attrs,
        Item::Static(i) => &i.attrs,
        Item::Struct(i) => &i.attrs,
        Item::Trait(i) => &i.attrs,
        Item::TraitAlias(i) => &i.attrs,
        Item::Type(i) => &i.attrs,
        Item::Union(i) => &i.attrs,
        Item::Use(i) => &i.attrs,
        _ => &[],
    }
}

/// `ImplItem`'s counterpart of [`item_attrs`], same `#[non_exhaustive]` posture.
fn impl_item_attrs(item: &ImplItem) -> &[Attribute] {
    match item {
        ImplItem::Const(i) => &i.attrs,
        ImplItem::Fn(i) => &i.attrs,
        ImplItem::Type(i) => &i.attrs,
        ImplItem::Macro(i) => &i.attrs,
        _ => &[],
    }
}

/// `TraitItem`'s counterpart of [`item_attrs`], same `#[non_exhaustive]` posture.
fn trait_item_attrs(item: &TraitItem) -> &[Attribute] {
    match item {
        TraitItem::Const(i) => &i.attrs,
        TraitItem::Fn(i) => &i.attrs,
        TraitItem::Type(i) => &i.attrs,
        TraitItem::Macro(i) => &i.attrs,
        _ => &[],
    }
}

#[cfg(test)]
mod tests;
