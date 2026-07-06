//! Store-binding IR fact — recognizes `storeName -> Prisma model name` bindings inside a domain's
//! STORES/Store file, projecting them as `zzop_core`-free `Vec<String>` model names. This is the
//! per-file substrate `zzop_engine`'s `assemble` now unions tree-wide into `SchemaUsage.bound_models`
//! (see `zzop_rules_schema::usage::cross_check_schema`'s `dead-model` rule), replacing the removed
//! `zzop_rules_schema::usage::scan_store_map`'s own `<root>/src/domains/**` filesystem re-walk — the
//! store-binding sibling of `db_table_consume.rs`'s `extract_query_call_sites`, which made the same move
//! for the schema x usage JOIN rules' query-call-site evidence.
//!
//! ## Recognized shapes
//! The same two binding patterns `scan_store_map` used to recognize, ported verbatim:
//! (a) `<name>: <factory>(...<getter>().<model>...)` — the STORES-object factory pattern.
//! (b) `(const|let) <name>Store = ...<getter>().<model>...` — the standalone-const pattern.
//! `<factory>` mirrors `zzop_parser_prisma::DEFAULT_STORE_FACTORY_FN` (`"createStore"`) as a local
//! literal — [`STORE_FACTORY_FN`] — same reasoning `db_table_consume::PRISMA_CLIENT_GETTER` already uses
//! to avoid a parser-typescript -> parser-prisma dependency edge for one string; `<getter>` reuses that
//! constant directly. Both `zzop_rules_schema::usage::scan_store_map`'s de-duplication rule (within one
//! file, the factory pattern is captured first and a standalone entry never overwrites an
//! already-claimed store name) and its plain-regex recognizer are ported as-is, not re-implemented via
//! the AST — the source text shape is simple enough that a regex stays faithful and avoids a second
//! AST-recognizer to keep in sync with `scan_store_map`'s old vocabulary.
//!
//! ## File-convention gating
//! `scan_store_map` only ever scanned files directly under `<root>/src/domains/<domain>/` whose filename
//! matched `STORES?\.ts$|[Ss]tore\.ts$`. The fused per-file pass has only one file's `rel` in hand at a
//! time (no directory listing to filter), so [`extract_store_bound_models`] reproduces that gating on
//! `rel` itself instead: a `/domains/` path segment anywhere, plus the same filename regex. Any file
//! that fails this gate yields no store bindings regardless of its text content — intrinsic to the
//! convention, faithful porting, not new coupling.

use std::sync::OnceLock;

use regex::Regex;

use crate::adapters::db_table_consume::PRISMA_CLIENT_GETTER;

/// Mirrors `zzop_parser_prisma::DEFAULT_STORE_FACTORY_FN` (`"createStore"`) — kept as a local literal
/// rather than a dependency on that crate, same reasoning as `db_table_consume::PRISMA_CLIENT_GETTER`.
const STORE_FACTORY_FN: &str = "createStore";

/// Extracts capitalized Prisma model names bound to a store IN THIS FILE — empty unless `rel` matches
/// the store-file convention (see module doc). Within one file, a store name claimed by the factory
/// pattern is never overwritten by a later standalone-pattern match of the same name (ported from
/// `scan_store_map`'s own de-duplication).
pub fn extract_store_bound_models(rel: &str, text: &str) -> Vec<String> {
    if !is_store_file(rel) {
        return Vec::new();
    }
    let mut by_store_name: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for c in factory_re().captures_iter(text) {
        by_store_name.insert(c[1].to_string(), capitalize(&c[2]));
    }
    for c in standalone_re().captures_iter(text) {
        by_store_name
            .entry(c[1].to_string())
            .or_insert_with(|| capitalize(&c[2]));
    }
    // Sorted so this file's cached bytes are stable across runs (a `HashMap` iterates nondeterministically);
    // the consumer unions into a `HashSet`, so order is output-irrelevant — this is a determinism convention,
    // matching the sorted `field_usage_tokens` sibling.
    let mut models: Vec<String> = by_store_name.into_values().collect();
    models.sort();
    models
}

/// `rel` contains a `/domains/` path segment and its filename matches the STORES/Store convention (see
/// module doc). NOTE: this is a deliberate SUPERSET of legacy `scan_store_map`, which only scanned files
/// directly under `<root>/src/domains/<domain>/`; here a `/domains/` segment anywhere (and any depth)
/// qualifies. Safe-direction — a broader store-file set can only grow `bound_models`, i.e. only ever
/// REMOVE `dead-model` findings, never add a false one.
fn is_store_file(rel: &str) -> bool {
    let has_domains_segment = rel.split('/').any(|seg| seg == "domains");
    let file_name = rel.rsplit('/').next().unwrap_or(rel);
    has_domains_segment && store_file_re().is_match(file_name)
}

fn store_file_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"STORES?\.ts$|[Ss]tore\.ts$").unwrap())
}

/// E.g. `itemStore: createStore(..., () => new PrismaStore(getPrisma().item))`.
fn factory_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"([A-Za-z0-9_]+):\s*{}[\s\S]*?{}\(\)\.([A-Za-z0-9_]+)",
            regex::escape(STORE_FACTORY_FN),
            regex::escape(PRISMA_CLIENT_GETTER)
        ))
        .unwrap()
    })
}

/// Standalone variant: `export const userStore = new PrismaStore(getPrisma().user)`.
fn standalone_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"(?:const|let)\s+([A-Za-z0-9_]+Store)\s*=[\s\S]*?{}\(\)\.([A-Za-z0-9_]+)",
            regex::escape(PRISMA_CLIENT_GETTER)
        ))
        .unwrap()
    })
}

/// First-char-uppercase (`item` -> `Item`) — mirrors `zzop_rules_schema::usage::capitalize` byte-for-byte
/// (that crate's own copy was deleted once `scan_store_map` moved here); duplicated locally rather than
/// shared, same reasoning `db_table_consume::capitalize` already documents for its own copy.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_file_both_shapes_yield_their_models() {
        let text = "import { createStore } from \"@app/store\";\nimport { PrismaStore, getPrisma } from \"@app/prisma\";\nexport const STORES = {\n  itemStore: createStore((f: any) => f, () => new PrismaStore(getPrisma().item)),\n};\nexport const userStore = new PrismaStore(getPrisma().user);\n";
        let mut models = extract_store_bound_models("src/domains/x/xStore.ts", text);
        models.sort();
        assert_eq!(models, vec!["Item".to_string(), "User".to_string()]);
    }

    #[test]
    fn non_store_file_yields_none() {
        let text = "import { createStore } from \"@app/store\";\nimport { getPrisma } from \"@app/prisma\";\nexport const itemStore = createStore(() => getPrisma().item);\n";
        assert!(extract_store_bound_models("src/domains/x/service.ts", text).is_empty());
    }

    #[test]
    fn store_file_outside_domains_yields_none() {
        let text = "import { PrismaStore, getPrisma } from \"@app/prisma\";\nexport const userStore = new PrismaStore(getPrisma().user);\n";
        assert!(extract_store_bound_models("src/shared/userStore.ts", text).is_empty());
    }

    #[test]
    fn stores_ts_factory_pattern_compound_camel_case_to_pascal_case() {
        let text = "import { createStore } from \"@app/store\";\nimport { PrismaStore, getPrisma } from \"@app/prisma\";\nexport const STORES = {\n  itemUserLimitStore: createStore((f: any) => f, () => new PrismaStore(getPrisma().itemUserLimit)),\n};\n";
        let models = extract_store_bound_models("src/domains/item/STORES.ts", text);
        assert_eq!(models, vec!["ItemUserLimit".to_string()]);
    }

    #[test]
    fn store_ts_mixed_case_variant_is_recognized() {
        let text = "import { PrismaStore, getPrisma } from \"@app/prisma\";\nexport const sessionStore = new PrismaStore(getPrisma().session);\n";
        let models = extract_store_bound_models("src/domains/session/Store.ts", text);
        assert_eq!(models, vec!["Session".to_string()]);
    }

    #[test]
    fn file_without_get_prisma_pattern_not_mapped() {
        let text = "import { JsonStore } from \"@app/json-store\";\nexport const STORES = {\n  postStore: new JsonStore(\"posts\"),\n};\n";
        assert!(extract_store_bound_models("src/domains/post/STORES.ts", text).is_empty());
    }

    #[test]
    fn same_store_name_twice_first_entry_wins() {
        let text = "import { createStore } from \"@app/store\";\nimport { PrismaStore, getPrisma } from \"@app/prisma\";\nexport const STORES = {\n  postStore: createStore((f: any) => f, () => new PrismaStore(getPrisma().post)),\n};\nexport const postStore = new PrismaStore(getPrisma().article);\n";
        // The factory pattern (post) claims `postStore` first; the standalone entry for the same name is
        // ignored, so `Article` never enters the result — mirrors `scan_store_map`'s own test.
        let models = extract_store_bound_models("src/domains/post/STORES.ts", text);
        assert_eq!(models, vec!["Post".to_string()]);
    }
}
