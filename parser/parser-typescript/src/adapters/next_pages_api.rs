//! Lexical scan for Next.js `pages/api` default-export handlers: `export default <expr>` is
//! invisible to `parse_symbols` (which only surfaces `ExportDefaultDecl`), and one handler serves
//! every HTTP method via `req.method` checks or a `defaultHandler({ GET: …, POST: … })` verb map.
//! `zpz-engine`'s file-convention route composition calls this scan to learn whether a candidate
//! file default-exports a handler and which methods its body names.
//!
//! Deliberately line-based/lexical, not AST-based. An empty `verbs` is an honest "statically
//! unknown", which the engine maps to its documented {GET, POST} fallback.

use std::sync::OnceLock;

use regex::Regex;

/// What `scan_pages_api_handler` learned about one candidate file.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PagesApiHandlerScan {
    /// 1-based line of the first non-comment `export default …` (or `export { x as default }`).
    /// `None` means the file has no default export (e.g. a config-only file).
    pub default_export_line: Option<u32>,
    /// Sorted, deduped UPPERCASE verbs named in the handler body: from `req.method`/`request.method`
    /// comparisons against a GET|POST|PUT|PATCH|DELETE literal (either operand order), plus
    /// `GET:`-style verb-map keys when the file uses the `defaultHandler(` idiom (bare verb keys
    /// elsewhere are too FP-prone to count). Comment lines are skipped.
    pub verbs: Vec<String>,
}

/// Scan one `pages/api` candidate file's text. Pure and allocation-light; never parses.
pub fn scan_pages_api_handler(text: &str) -> PagesApiHandlerScan {
    PagesApiHandlerScan {
        default_export_line: find_default_export_line(text),
        verbs: collect_verbs(text),
    }
}

fn is_comment_line(line: &str) -> bool {
    line.trim_start().starts_with("//")
}

/// First non-comment line with any `export default` form (bare identifier, function, wrapped
/// call) or an `export { x as default }` re-export.
fn find_default_export_line(text: &str) -> Option<u32> {
    static DEFAULT_EXPORT: OnceLock<Regex> = OnceLock::new();
    static REEXPORT_AS_DEFAULT: OnceLock<Regex> = OnceLock::new();
    let default_export =
        DEFAULT_EXPORT.get_or_init(|| Regex::new(r"\bexport\s+default\b").unwrap());
    let reexport_as_default = REEXPORT_AS_DEFAULT
        .get_or_init(|| Regex::new(r"export\s*\{[^}]*\bas\s+default\b").unwrap());

    for (idx, raw) in text.lines().enumerate() {
        if is_comment_line(raw) {
            continue;
        }
        if default_export.is_match(raw) || reexport_as_default.is_match(raw) {
            return Some((idx + 1) as u32);
        }
    }
    None
}

/// Sorted, deduped UPPERCASE verbs — see [`PagesApiHandlerScan::verbs`] for the collection rules.
fn collect_verbs(text: &str) -> Vec<String> {
    static METHOD_CMP_FORWARD: OnceLock<Regex> = OnceLock::new();
    static METHOD_CMP_REVERSE: OnceLock<Regex> = OnceLock::new();
    static VERB_KEY: OnceLock<Regex> = OnceLock::new();

    // `req.method`/`request.method` compared to a quoted verb literal, either operand order.
    let forward = METHOD_CMP_FORWARD.get_or_init(|| {
        Regex::new(
            r#"(?:req|request)\.method\s*(?:===|!==|==|!=)\s*['"](GET|POST|PUT|PATCH|DELETE)['"]"#,
        )
        .unwrap()
    });
    let reverse = METHOD_CMP_REVERSE.get_or_init(|| {
        Regex::new(
            r#"['"](GET|POST|PUT|PATCH|DELETE)['"]\s*(?:===|!==|==|!=)\s*(?:req|request)\.method"#,
        )
        .unwrap()
    });
    // Verb-map object key (`GET:`, `"GET":`, `'GET':`), only consulted when `defaultHandler(` appears.
    let verb_key =
        VERB_KEY.get_or_init(|| Regex::new(r#"\b(GET|POST|PUT|PATCH|DELETE)\b['"]?\s*:"#).unwrap());

    let has_verb_map_idiom = text.contains("defaultHandler(");

    let mut verbs: Vec<String> = Vec::new();
    for raw in text.lines() {
        if is_comment_line(raw) {
            continue;
        }
        for cap in forward.captures_iter(raw) {
            push_unique_verb(&mut verbs, &cap[1]);
        }
        for cap in reverse.captures_iter(raw) {
            push_unique_verb(&mut verbs, &cap[1]);
        }
        if has_verb_map_idiom {
            for cap in verb_key.captures_iter(raw) {
                push_unique_verb(&mut verbs, &cap[1]);
            }
        }
    }
    verbs.sort();
    verbs
}

fn push_unique_verb(verbs: &mut Vec<String>, verb: &str) {
    if !verbs.iter().any(|v| v == verb) {
        verbs.push(verb.to_string());
    }
}

#[cfg(test)]
mod tests {
    //! Coverage for `scan_pages_api_handler`: default-export detection, verb extraction (both
    //! comparison orders, the `defaultHandler(` verb-map idiom, and the bare-verb-key FP guard).
    use super::*;

    #[test]
    fn bare_default_export_detected_not_on_line_one() {
        let text = "import handler from './handler';\n\nexport default handler;\n";
        let scan = scan_pages_api_handler(text);
        assert_eq!(scan.default_export_line, Some(3));
        assert!(scan.verbs.is_empty());
    }

    #[test]
    fn wrapped_default_export_call_detected() {
        let text = "export default defaultResponder(handler, \"/api/book/event\");\n";
        let scan = scan_pages_api_handler(text);
        assert_eq!(scan.default_export_line, Some(1));
    }

    #[test]
    fn reexport_as_default_detected() {
        let text = "function handler() {}\nexport { handler as default };\n";
        let scan = scan_pages_api_handler(text);
        assert_eq!(scan.default_export_line, Some(2));
    }

    #[test]
    fn config_only_file_has_no_default_export() {
        let text = "export const config = {\n  api: { bodyParser: false },\n};\n";
        let scan = scan_pages_api_handler(text);
        assert_eq!(scan.default_export_line, None);
    }

    #[test]
    fn method_not_equal_string_comparison() {
        let text = "export default function handler(req, res) {\n  if (req.method !== \"POST\") return;\n}\n";
        let scan = scan_pages_api_handler(text);
        assert_eq!(scan.verbs, vec!["POST".to_string()]);
    }

    #[test]
    fn reversed_operand_order_strict_equal() {
        let text = "if (\"DELETE\" === req.method) {}\n";
        let scan = scan_pages_api_handler(text);
        assert_eq!(scan.verbs, vec!["DELETE".to_string()]);
    }

    #[test]
    fn request_dot_method_single_quoted() {
        let text = "if (request.method === 'PUT') {}\n";
        let scan = scan_pages_api_handler(text);
        assert_eq!(scan.verbs, vec!["PUT".to_string()]);
    }

    #[test]
    fn multiple_method_checks_sorted_and_deduped() {
        let text = "if (req.method === \"GET\") {}\nif (req.method === \"POST\") {}\nif (req.method === \"GET\") {}\n";
        let scan = scan_pages_api_handler(text);
        assert_eq!(scan.verbs, vec!["GET".to_string(), "POST".to_string()]);
    }

    #[test]
    fn default_handler_verb_map_idiom() {
        let text =
            "export default defaultHandler({ GET: import(\"./x\"), POST: import(\"./y\") });\n";
        let scan = scan_pages_api_handler(text);
        assert_eq!(scan.verbs, vec!["GET".to_string(), "POST".to_string()]);
        assert_eq!(scan.default_export_line, Some(1));
    }

    #[test]
    fn bare_verb_key_without_default_handler_contributes_nothing() {
        let text = "const routes = {\n  GET: handleGet,\n  POST: handlePost,\n};\n";
        let scan = scan_pages_api_handler(text);
        assert!(scan.verbs.is_empty());
    }

    #[test]
    fn comment_line_default_export_ignored() {
        let text = "// export default handler\n";
        let scan = scan_pages_api_handler(text);
        assert_eq!(scan.default_export_line, None);
    }

    #[test]
    fn lowercase_or_non_verb_object_keys_ignored() {
        let text =
            "export default defaultHandler({\n  get: handleGet,\n  TARGET: something,\n});\n";
        let scan = scan_pages_api_handler(text);
        assert!(scan.verbs.is_empty());
    }
}
