//! hono/client typed-RPC CONSUME extractor — projects the client-side calls a TS/JS tree makes
//! through Hono's `hc<AppType>()` typed proxy client, so the core cross-layer linker can join each
//! call to its backend Hono route. Unlike `trpc_consume` (whose dotted procedure path IS the join
//! key), a hono/client call chain only names the ROUTE TAIL, so this module also resolves the BASE
//! PATH the client was constructed with, once per file — kind `"http"`, keyed via
//! `http_consume_interface_key` like `egress`'s FE HTTP-call extractor.
//!
//! Only files that import `hc` from a specifier containing `hono/client` (case-insensitively) are
//! scanned — a bare `.signout.$post()`-shaped member call is otherwise far too generic to key.
//!
//! ## Base-path resolution (from the FIRST `hc(...)`/`hc<T>(...)` call in the file)
//! A string literal starting with `/` is the base verbatim; a full external URL contributes its path
//! part only. A template literal concatenates quasis with each interpolation as `{}`, then takes the
//! substring from the first `/` (`None` if that still contains `{}`). An identifier or one-level
//! member access (e.g. `options.baseUrl`) is traced ONE hop: if the call sits inside a class
//! instantiated elsewhere with an object literal carrying the same property name
//! (`new SomeClient({ baseUrl: <expr> })`), the same rules apply to `<expr>`; any other shape leaves
//! the base unresolved.
//!
//! An unresolved base does NOT skip the file — every recognized call chain is still extracted, just
//! emitted UNRESOLVED (`key: None`, `raw: Some("<chain> $verb")`, `method: Some(VERB)`), the same
//! honest "seen but unkeyed" shape `egress`'s dynamic-URL consumes use.
//!
//! ## Call-chain recognition
//! A "hc-derived receiver" is a local binding via `const client = hc<T>(...)` or
//! `this.<field> = hc<T>(...)` inside a class — a class wrapping Hono's client factory, the shape a
//! generated API client SDK commonly uses. A call chain is a member-access sequence rooted at a
//! recognized receiver, each link a plain `.ident` or a bracket string literal (a Hono `:param`
//! segment kept verbatim), ending in a terminal `.$get()`/`.$post()`/`.$put()`/`.$patch()`/`.$delete()`
//! call. A `.param(...)`/`.query(...)` link anywhere is transparently skipped; any other embedded
//! call, or a non-literal computed segment, aborts the whole chain.

use std::collections::HashSet;

use swc_core::common::SourceMap;
use swc_core::ecma::visit::VisitWith;
use zzop_core::IoConsume;

mod consume;
mod scan;
#[cfg(test)]
mod tests;

use consume::ConsumeCollector;
use scan::{ClientScanner, NewExprPropFinder};

/// Extract hono/client typed-RPC CONSUME entries from one file.
pub fn extract_hono_client_consumes(rel: &str, text: &str) -> Vec<IoConsume> {
    let mut hc_idents: HashSet<String> = HashSet::new();
    for (local, binding) in crate::parse_imports(rel, text) {
        if binding.original == "hc"
            && binding
                .specifier
                .to_ascii_lowercase()
                .contains("hono/client")
        {
            hc_idents.insert(local);
        }
    }
    if hc_idents.is_empty() {
        return Vec::new();
    }

    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };

    let mut scanner = ClientScanner {
        hc_idents: &hc_idents,
        class_stack: Vec::new(),
        ident_receivers: HashSet::new(),
        this_field_receivers: HashSet::new(),
        first_call: None,
        hc_call_count: 0,
    };
    module.visit_with(&mut scanner);

    // Multi-client guard: with 2+ `hc()` calls in one file there is no single base to attribute a
    // chain to — keying every receiver with the FIRST call's base would silently mis-key the
    // others, so the base is treated as non-static and every chain emits UNRESOLVED instead.
    let base: Option<String> = if scanner.hc_call_count > 1 {
        None
    } else {
        scanner.first_call.and_then(|fc| {
            fc.resolved.or_else(|| {
                let (prop_name, class_name) = fc.trace?;
                let mut finder = NewExprPropFinder {
                    class_name: &class_name,
                    prop_name: &prop_name,
                    result: None,
                };
                module.visit_with(&mut finder);
                finder.result
            })
        })
    };

    let cm_ref: &SourceMap = &cm;
    let mut collector = ConsumeCollector {
        cm: cm_ref,
        file: rel,
        ident_receivers: &scanner.ident_receivers,
        this_field_receivers: &scanner.this_field_receivers,
        base: &base,
        out: Vec::new(),
    };
    module.visit_with(&mut collector);
    collector.out
}
