//! `cross-layer/unconsumed-procedure` (info) — one finding per `CrossLayerResult::unconsumed_provides` entry
//! of kind `"trpc"`: a tRPC procedure no source in this `analyzeTrees` run calls. Mirrors
//! `unconsumed_endpoint`'s reasoning, but for the `trpc` key vocabulary the router-fragment composition assembles.
//!
//! ## Why this rule exists
//! TypeScript's compiler guarantees the OPPOSITE direction — a call site naming a nonexistent procedure is
//! a compile error — but gives no signal for a procedure that exists and is never called, since an unused
//! router entry is perfectly well-typed. That asymmetry is exactly the gap this rule fills.
//! Provider sites in test-path files (`zzop_core::is_test_file`) are skipped — a test-only router
//! isn't deployed surface.

use zzop_core::io::TaggedProvide;
use zzop_core::{Finding, Severity};

use super::split_key;

pub fn unconsumed_procedure_findings(unconsumed_provides: &[TaggedProvide]) -> Vec<Finding> {
    let mut out: Vec<Finding> = unconsumed_provides
        .iter()
        .filter(|p| p.provide.kind == "trpc" && !zzop_core::is_test_file(&p.provide.file))
        .filter_map(|p| {
            let key = &p.provide.key;
            let (verb, procedure_path) = split_key(key)?;
            let message = format!(
                "tRPC procedure `{key}` (source `{}`) is defined but no analyzed source calls it. That said, \
                 this analysis cannot see every consumer: a server-side `createCaller` invocation, an SSR \
                 helper that calls the procedure directly instead of through a hook, `useUtils`-based cache \
                 access (prefetch/invalidate/setData calls that reference the procedure without a normal \
                 `.query()`/`.mutate()` call site), or a consumer in a repo outside this `analyzeTrees` run \
                 may still call it. TypeScript's compiler guarantees the inverse direction — a call site \
                 naming a procedure that doesn't exist is a compile error — but gives no signal at all for \
                 an unused definition, since a never-called router entry is perfectly well-typed; that gap \
                 is exactly why this rule exists. Confirm no out-of-analysis consumer before deleting the \
                 procedure — dead API surface is both maintenance burden and attack surface, reachable by \
                 anyone who finds it even with no legitimate caller left. Disable via rule config \
                 `disabled_rules: [\"cross-layer/unconsumed-procedure\"]` if this source's procedures are \
                 consumed by clients outside static analysis on purpose.",
                p.source
            );
            Some(Finding {
                rule_id: "cross-layer/unconsumed-procedure".to_string(),
                severity: Severity::Info,
                file: p.provide.file.clone(),
                line: p.provide.line,
                message,
                data: Some(serde_json::json!({
                    "key": key,
                    "source": p.source,
                    "verb": verb,
                    "procedurePath": procedure_path,
                    "symbol": p.provide.symbol,
                })),
            })
        })
        .collect();
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use zzop_core::io::IoProvide;

    fn dead(
        kind: &str,
        key: &str,
        source: &str,
        file: &str,
        line: u32,
        symbol: Option<&str>,
    ) -> TaggedProvide {
        TaggedProvide {
            source: source.to_string(),
            provide: IoProvide {
                kind: kind.to_string(),
                key: key.to_string(),
                file: file.to_string(),
                line,
                symbol: symbol.map(str::to_string),
            },
        }
    }

    #[test]
    fn dead_trpc_provide_is_flagged_with_source_and_anchor() {
        let out = unconsumed_procedure_findings(&[dead(
            "trpc",
            "QUERY viewer.bookings.get",
            "web",
            "viewer/_router.ts",
            42,
            Some("bookingsRouter.get"),
        )]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/unconsumed-procedure");
        assert_eq!(out[0].severity, Severity::Info);
        assert_eq!(out[0].file, "viewer/_router.ts");
        assert_eq!(out[0].line, 42);
        assert!(out[0].message.contains("QUERY viewer.bookings.get"));
        assert!(out[0].message.contains("source `web`"));
        assert!(out[0].message.contains("createCaller"));
        assert!(out[0].message.contains("useUtils"));
        assert!(out[0].message.contains("disabled_rules"));
    }

    #[test]
    fn http_kind_dead_provide_is_not_this_rules_turf() {
        let out = unconsumed_procedure_findings(&[dead(
            "http",
            "GET /api/users",
            "be",
            "Api.java",
            12,
            None,
        )]);
        assert!(out.is_empty());
    }

    #[test]
    fn dead_provide_registered_in_a_test_fixture_file_is_skipped() {
        let out = unconsumed_procedure_findings(&[dead(
            "trpc",
            "MUTATION admin.users.delete",
            "web",
            "src/__tests__/router.test.ts",
            5,
            None,
        )]);
        assert!(out.is_empty());
    }

    #[test]
    fn determinism_multiple_findings_sorted_by_file_then_line() {
        let out = unconsumed_procedure_findings(&[
            dead("trpc", "QUERY b.get", "web", "z.ts", 1, None),
            dead("trpc", "QUERY a.get", "web", "a.ts", 9, None),
            dead("trpc", "MUTATION a.create", "web", "a.ts", 2, None),
        ]);
        let sites: Vec<(&str, u32)> = out.iter().map(|f| (f.file.as_str(), f.line)).collect();
        assert_eq!(sites, vec![("a.ts", 2), ("a.ts", 9), ("z.ts", 1)]);
    }

    #[test]
    fn data_payload_shape_carries_key_source_verb_path_and_symbol() {
        let out = unconsumed_procedure_findings(&[dead(
            "trpc",
            "MUTATION viewer.bookings.create",
            "web",
            "viewer/_router.ts",
            10,
            Some("bookingsRouter.create"),
        )]);
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["key"], "MUTATION viewer.bookings.create");
        assert_eq!(data["source"], "web");
        assert_eq!(data["verb"], "MUTATION");
        assert_eq!(data["procedurePath"], "viewer.bookings.create");
        assert_eq!(data["symbol"], "bookingsRouter.create");
    }

    #[test]
    fn data_payload_symbol_is_absent_when_none() {
        let out = unconsumed_procedure_findings(&[dead(
            "trpc",
            "QUERY viewer.get",
            "web",
            "viewer/_router.ts",
            10,
            None,
        )]);
        let data = out[0].data.as_ref().unwrap();
        assert!(data["symbol"].is_null());
    }
}
