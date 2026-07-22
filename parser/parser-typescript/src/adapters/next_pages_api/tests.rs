//! Coverage for `scan_pages_api_handler`: default-export detection (inline fn/arrow, ident-resolved,
//! re-exported), verb extraction scoped to the resolved handler's body (both comparison orders,
//! any request-parameter name, method-discriminated `switch`/`case` labels, the `defaultHandler(`
//! verb-map idiom), and the FP guards (bare-verb-key, unrelated switch, and a verb check OUTSIDE the
//! handler's body).
use super::*;

/// `pages/api` files are `.ts`/`.js`; a fixed `.ts` rel drives extension-based syntax selection.
fn scan(text: &str) -> PagesApiHandlerScan {
    scan_pages_api_handler("pages/api/x.ts", text)
}

#[test]
fn bare_default_export_detected_not_on_line_one() {
    let text = "import handler from './handler';\n\nexport default handler;\n";
    let scan = scan(text);
    assert_eq!(scan.default_export_line, Some(3));
    assert!(scan.verbs.is_empty());
}

#[test]
fn wrapped_default_export_call_detected() {
    let text = "export default defaultResponder(handler, \"/api/book/event\");\n";
    let scan = scan(text);
    assert_eq!(scan.default_export_line, Some(1));
}

#[test]
fn reexport_as_default_detected() {
    let text = "function handler() {}\nexport { handler as default };\n";
    let scan = scan(text);
    assert_eq!(scan.default_export_line, Some(2));
}

#[test]
fn config_only_file_has_no_default_export() {
    let text = "export const config = {\n  api: { bodyParser: false },\n};\n";
    let scan = scan(text);
    assert_eq!(scan.default_export_line, None);
}

#[test]
fn method_not_equal_string_comparison() {
    let text =
        "export default function handler(req, res) {\n  if (req.method !== \"POST\") return;\n}\n";
    let scan = scan(text);
    assert_eq!(scan.verbs, vec!["POST".to_string()]);
}

#[test]
fn reversed_operand_order_strict_equal() {
    let text =
        "export default function handler(req, res) {\n  if (\"DELETE\" === req.method) {}\n}\n";
    let scan = scan(text);
    assert_eq!(scan.verbs, vec!["DELETE".to_string()]);
}

#[test]
fn request_dot_method_single_quoted() {
    let text =
        "export default function handler(request, res) {\n  if (request.method === 'PUT') {}\n}\n";
    let scan = scan(text);
    assert_eq!(scan.verbs, vec!["PUT".to_string()]);
}

#[test]
fn multiple_method_checks_sorted_and_deduped() {
    let text = "export default function handler(req, res) {\n  if (req.method === \"GET\") {}\n  if (req.method === \"POST\") {}\n  if (req.method === \"GET\") {}\n}\n";
    let scan = scan(text);
    assert_eq!(scan.verbs, vec!["GET".to_string(), "POST".to_string()]);
}

#[test]
fn arrow_fn_default_export_collects_body_verbs() {
    let text = "export default (req, res) => {\n  if (req.method === 'GET') return;\n};\n";
    let scan = scan(text);
    assert_eq!(scan.default_export_line, Some(1));
    assert_eq!(scan.verbs, vec!["GET".to_string()]);
}

#[test]
fn function_decl_default_export_collects_body_verbs() {
    let text =
        "export default function handler(req, res) {\n  if (req.method === 'PATCH') return;\n}\n";
    let scan = scan(text);
    assert_eq!(scan.default_export_line, Some(1));
    assert_eq!(scan.verbs, vec!["PATCH".to_string()]);
}

#[test]
fn default_handler_verb_map_idiom() {
    let text = "export default defaultHandler({ GET: import(\"./x\"), POST: import(\"./y\") });\n";
    let scan = scan(text);
    assert_eq!(scan.verbs, vec!["GET".to_string(), "POST".to_string()]);
    assert_eq!(scan.default_export_line, Some(1));
}

#[test]
fn bare_verb_key_without_default_handler_contributes_nothing() {
    let text =
        "const routes = {\n  GET: handleGet,\n  POST: handlePost,\n};\nexport default routes;\n";
    let scan = scan(text);
    assert!(scan.verbs.is_empty());
}

#[test]
fn comment_line_default_export_ignored() {
    let text = "// export default handler\n";
    let scan = scan(text);
    assert_eq!(scan.default_export_line, None);
}

#[test]
fn lowercase_or_non_verb_object_keys_ignored() {
    let text = "export default defaultHandler({\n  get: handleGet,\n  TARGET: something,\n});\n";
    let scan = scan(text);
    assert!(scan.verbs.is_empty());
}

#[test]
fn switch_on_req_method_collects_case_label_verbs() {
    let text = "export default function handler(req, res) {\n  switch (req.method) {\n    case 'POST':\n      return create(req, res);\n    case 'DELETE':\n      return remove(req, res);\n  }\n}\n";
    let scan = scan(text);
    assert_eq!(scan.verbs, vec!["DELETE".to_string(), "POST".to_string()]);
}

#[test]
fn switch_on_request_method_double_quoted_case() {
    let text =
        "export default function handler(request, res) {\n  switch (request.method) {\n    case \"PUT\": break;\n  }\n}\n";
    let scan = scan(text);
    assert_eq!(scan.verbs, vec!["PUT".to_string()]);
}

#[test]
fn case_labels_without_a_method_switch_contribute_nothing() {
    // A switch on something other than the request method must not mint verbs from its case labels.
    let text = "switch (action) {\n  case 'POST':\n    return x;\n}\n";
    let scan = scan(text);
    assert!(scan.verbs.is_empty());
}

#[test]
fn unrelated_verb_switch_does_not_over_collect() {
    // The documented FP, now fixed: a handler that switches on `req.method` for GET/POST while an
    // unrelated `switch (action) { case 'DELETE': }` coexists in the SAME handler body. The old
    // line scan flipped `has_method_switch` on and then swept every `case 'VERB':` in the file,
    // over-collecting DELETE. The AST scopes case labels to the method-discriminated switch, so
    // only GET/POST are collected.
    let text = "export default function handler(req, res) {\n  const action = req.query.action;\n  switch (action) {\n    case 'DELETE':\n      return audit();\n  }\n  switch (req.method) {\n    case 'GET':\n      return read(req, res);\n    case 'POST':\n      return write(req, res);\n  }\n}\n";
    let scan = scan(text);
    assert_eq!(scan.verbs, vec!["GET".to_string(), "POST".to_string()]);
}

#[test]
fn verb_check_in_a_helper_above_the_handler_is_not_collected() {
    // The Class-B FP the whole-module walk still had: a `req.method` comparison in an unrelated
    // top-level helper (never called by, and not part of, the default-exported handler) must NOT
    // contribute a verb to the route — only the handler's OWN body is in scope.
    let text = "function auditHelper(req) {\n  if (req.method === 'DELETE') {\n    audit();\n  }\n}\nexport default function handler(req, res) {\n  if (req.method === 'GET') return read(req, res);\n}\n";
    let scan = scan(text);
    assert_eq!(scan.verbs, vec!["GET".to_string()]);
}

#[test]
fn request_parameter_named_neither_req_nor_request_is_recognized() {
    // The request-object identifier is the handler's ACTUAL first parameter name, never a hardcoded
    // `req`/`request` — a differently-named parameter must still be witnessed.
    let text =
        "export default function handler(r, res) {\n  if (r.method === 'PATCH') return;\n}\n";
    let scan = scan(text);
    assert_eq!(scan.verbs, vec!["PATCH".to_string()]);
}

#[test]
fn ident_default_export_resolves_to_same_file_top_level_function() {
    // `export default handler;` where `handler` is declared earlier in the SAME file (not imported)
    // resolves to that binding's body, unlike the imported case in
    // `bare_default_export_detected_not_on_line_one`.
    let text = "function handler(req, res) {\n  if (req.method === 'GET') return;\n}\nexport default handler;\n";
    let scan = scan(text);
    assert_eq!(scan.default_export_line, Some(4));
    assert_eq!(scan.verbs, vec!["GET".to_string()]);
}

#[test]
fn ident_default_export_resolves_to_same_file_top_level_const_arrow() {
    let text = "const handler = (req, res) => {\n  if (req.method === 'DELETE') return;\n};\nexport default handler;\n";
    let scan = scan(text);
    assert_eq!(scan.verbs, vec!["DELETE".to_string()]);
}

#[test]
fn non_literal_method_comparison_is_ignored() {
    // `req.method` compared against a non-literal (a variable) is never guessed at.
    let text = "export default function handler(req, res) {\n  const expected = getExpectedMethod();\n  if (req.method === expected) return;\n}\n";
    let scan = scan(text);
    assert!(scan.verbs.is_empty());
}

#[test]
fn unparseable_text_yields_default_scan() {
    let scan = scan("export default function( {{{ not valid js at all");
    assert_eq!(scan, PagesApiHandlerScan::default());
}
