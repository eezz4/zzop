//! tRPC ROUTER-FRAGMENT extractor — projects the STATIC shape of every top-level `router({...})` /
//! `createTRPCRouter({...})` initializer in one TS file into a [`ProcedureRouterFragment`]: an ordered list of
//! [`ProcedureRouterEntry`] describing what each object key resolves to (a leaf procedure, an inline nested
//! router, or a reference to a sub-router by identifier).
//!
//! ## Why a fragment, not an `IoProvide`
//! A tRPC router composes across files: `viewerRouter` typically imports `bookingsRouter` from
//! `./bookings/_router` and re-mounts it under a key, so the FULL route path for a leaf several
//! sub-routers deep (`viewerRouter.bookings.create` -> key path `bookings.create`) is only knowable
//! once every file's fragment has been assembled together. This module reports exactly what THIS
//! file's text says, as a small ordered tree, and leaves the cross-file composition to the engine's
//! assembly pass.
//!
//! ## Recognized vocabulary (v1)
//! Router factory callee: a plain identifier call, `router(...)` or `createTRPCRouter(...)`, matched
//! by lexical name only, captured from `const <name> = router({...})` at module top level after
//! unwrapping `as`/`(...)`/`satisfies`/`!` wrappers. Object keys are a plain identifier or
//! string-literal key; a **computed key** (`[someExpr()]: ...`) skips just that one entry. A property
//! value is classified in this order: (1) a bare identifier -> [`ProcedureRouterEntry::Ref`] (`specifier` =
//! `Some(source)` when the ident is one of this file's own import bindings, `None` otherwise —
//! assumed a same-file local router); (2) a call to `router(...)`/`createTRPCRouter(...)` ->
//! [`ProcedureRouterEntry::Nested`], recursing the same way; (3) a builder-chain call with a
//! `query`/`mutation`/`subscription` member call anywhere down the chain ->
//! [`ProcedureRouterEntry::Leaf`] (e.g. `authedProcedure.input(z...).use(mw).mutation(fn)` is MUTATION);
//! (4) anything else is skipped.
//!
//! `mergeRouters(a, b, ...)` as the top-level initializer produces one
//! `ProcedureRouterEntry::Ref { key: String::new(), ident, specifier }` per bare-identifier argument, in
//! argument order — the empty key signals "splice this sub-router's entries in here" (there is no
//! key: `mergeRouters` flattens its arguments into ONE namespace). A non-identifier argument is
//! skipped, not recursed into — v1 only recognizes a plain sub-router
//! `mergeRouters(fooRouter, barRouter)` call. A top-level `const` whose initializer is neither a
//! router factory nor `mergeRouters(...)` produces no fragment at all.
//!
//! Object-literal spread properties (`...someSubRouter`) are not expanded into entries — skipped, same
//! "never guess" stance as everything else above. Shorthand object properties (`{ bookings }`) ARE
//! supported as a `Ref` to the same-named identifier.

mod extract;

pub use extract::extract_procedure_router_fragments;

#[cfg(test)]
mod tests;
