use super::*;

fn vocab() -> RustGuardVocab<'static> {
    RustGuardVocab {
        optional_extractor_prefixes: RUST_OPTIONAL_EXTRACTOR_PREFIXES,
    }
}

fn callees(rel: &str, src: &str) -> Vec<String> {
    parse_extractor_guards(rel, src, &vocab())
        .into_iter()
        .map(|c| c.callee_name)
        .collect()
}

/// The exact shape measured on `corpus/oss/be-axum` — the one this module exists for.
#[test]
fn an_extractor_typed_parameter_becomes_an_edge_out_of_the_handler() {
    let src = "async fn create_article(auth_user: AuthUser, ctx: Extension<ApiContext>) {}\n";
    let calls = parse_extractor_guards("a.rs", src, &vocab());
    let names: Vec<&str> = calls.iter().map(|c| c.callee_name.as_str()).collect();
    assert!(names.contains(&"AuthUser"), "{names:?}");
    assert!(
        calls.iter().all(|c| c.from_symbol == "a.rs#create_article"),
        "every signature edge belongs to its own handler: {calls:?}"
    );
}

/// The veto half. `MaybeAuthUser` CONTAINS `auth` and would clear a route through the rule's substring
/// vocabulary, but it admits anonymous callers — so it must never reach the graph at all.
#[test]
fn an_optional_extractor_is_vetoed_before_it_can_clear_a_route() {
    let names = callees("a.rs", "async fn get_article(u: MaybeAuthUser) {}\n");
    assert!(
        !names.iter().any(|n| n == "MaybeAuthUser"),
        "an optional extractor is not a gate: {names:?}"
    );
}

/// The veto is DECLARED, not built in: a project spelling its optional extractor another way says so in
/// config, and a name outside the declared list is emitted normally.
#[test]
fn the_veto_list_is_the_vocabularys_and_nothing_else() {
    let src = "async fn h(u: PerhapsAuthUser) {}\n";
    assert!(callees("a.rs", src).contains(&"PerhapsAuthUser".to_string()));
    let declared = RustGuardVocab {
        optional_extractor_prefixes: &["perhaps"],
    };
    let names: Vec<String> = parse_extractor_guards("a.rs", src, &declared)
        .into_iter()
        .map(|c| c.callee_name)
        .collect();
    assert!(names.is_empty(), "{names:?}");
}

#[test]
fn a_generic_argument_is_emitted_alongside_its_container() {
    let names = callees("a.rs", "async fn h(u: Extension<CurrentUser>) {}\n");
    assert!(names.contains(&"Extension".to_string()), "{names:?}");
    assert!(names.contains(&"CurrentUser".to_string()), "{names:?}");
}

#[test]
fn a_reference_parameter_is_unwrapped_rather_than_dropped() {
    assert_eq!(callees("a.rs", "fn h(u: &AuthUser) {}\n"), vec!["AuthUser"]);
}

/// The symbol id must agree byte-for-byte with `lang::symbols`/`lang::calls`, or the edge dangles.
#[test]
fn an_impl_method_uses_the_shared_symbol_id_shape() {
    let src = "struct H;\nimpl H {\n    async fn create(&self, u: AuthUser) {}\n}\n";
    let calls = parse_extractor_guards("a.rs", src, &vocab());
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0].from_symbol, "a.rs#H.create");
    let ids: Vec<String> = crate::lang::symbols::parse_symbols("a.rs", src)
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert!(ids.contains(&"a.rs#H.create".to_string()), "got: {ids:?}");
}

#[test]
fn a_self_receiver_produces_no_edge() {
    let calls = parse_extractor_guards(
        "a.rs",
        "struct H;\nimpl H {\n    fn f(&self) {}\n}\n",
        &vocab(),
    );
    assert!(calls.is_empty(), "{calls:?}");
}

#[test]
fn an_unparseable_file_yields_nothing_rather_than_panicking() {
    assert!(parse_extractor_guards("a.rs", "fn f(:\n", &vocab()).is_empty());
}

/// The seam this module was the last dissenter on. A guard extracted from inside an inline `mod`
/// borrows the symbol id a top-level item of the same name would have, because `parse_symbols` mints
/// no symbol for a nested item — so it clears a route it never guards.
///
/// Measured end-to-end on 2026-08-11 before this arm was removed: a tree whose deployed
/// `create_user(body: String)` checks nothing was flagged by `mutating-route-no-auth`, and adding an
/// ordinary `#[cfg(test)] mod tests { async fn create_user(user: AuthUser) {} }` silenced it — with no
/// warning, blindSpot or disclosure class saying so. `lang::calls`'s module doc records the same
/// premise being measured false on its own axis first; this pins that the two files now agree.
#[test]
fn a_guard_inside_an_inline_mod_never_becomes_an_edge_out_of_a_top_level_homonym() {
    let src = "\
pub async fn create_user(body: String) -> String { body }
#[cfg(test)]
mod tests {
    async fn create_user(user: AuthUser) {}
}
";
    // The top-level handler's own parameter type still rides — this matcher emits every parameter
    // type and lets the engine's vocabulary decide what counts as a guard. What must NOT appear is
    // `AuthUser`, which exists only inside the nested module.
    assert_eq!(
        callees("a.rs", src),
        vec!["String"],
        "a nested homonym must contribute no guard edge — it would ride the top-level symbol's id"
    );

    // The control that makes the assertion above mean something: the SAME parameter at top level does
    // produce the edge, so its absence is a scope decision and not a broken extractor.
    assert_eq!(
        callees("a.rs", "async fn create_user(user: AuthUser) {}\n"),
        vec!["AuthUser"]
    );
}
