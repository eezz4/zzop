use super::{scan, snippet};

#[test]
fn fetch_inside_for_of_loop_is_flagged() {
    let f = scan(
        "svc.ts",
        "declare const ids: string[];\nexport async function f() {\n  for (const id of ids) {\n    const r = await fetch(\"/api/\" + id);\n    console.log(r);\n  }\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(snippet(&f[0]).contains("fetch"));
}

#[test]
fn axios_get_inside_traditional_for_loop_is_flagged() {
    let f = scan(
        "svc.ts",
        "declare const axios: any;\ndeclare const ids: string[];\nexport async function f() {\n  for (let i = 0; i < ids.length; i++) {\n    await axios.get(\"/u/\" + ids[i]);\n  }\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(snippet(&f[0]).contains("axios.get"));
}

#[test]
fn fetch_inside_foreach_callback_is_flagged() {
    let f = scan(
        "svc.ts",
        "declare const items: any[];\nexport function f() {\n  items.forEach((it) => {\n    fetch(\"/track/\" + it.id);\n  });\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

#[test]
fn single_fetch_outside_any_loop_is_not_flagged() {
    let f = scan(
        "svc.ts",
        "export async function f(id: string) {\n  return await fetch(\"/api/\" + id);\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn in_memory_map_get_inside_loop_is_not_a_network_call() {
    let f = scan(
        "svc.ts",
        "declare const cacheMap: Map<string, number>;\ndeclare const ids: string[];\nexport function f() {\n  for (const id of ids) {\n    const v = cacheMap.get(id);\n    console.log(v);\n  }\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

/// A top-level-function variant of a `class S { async f() { ... this.httpService.get(...) } }` shape,
/// exercising the broadened receiver vocabulary (`httpService.get`) this rule targets.
#[test]
fn broadened_receiver_vocab_httpservice_get_inside_loop_is_flagged() {
    let f = scan(
        "svc.ts",
        "declare const ids: string[];\nexport async function f() {\n  const httpService: any = null;\n  for (const id of ids) {\n    await httpService.get(\"/u/\" + id);\n  }\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(snippet(&f[0]).contains("httpService.get"));
}

#[test]
fn ofetch_bare_client_inside_loop_is_flagged() {
    let f = scan(
        "svc.ts",
        "declare const ofetch: any;\ndeclare const ids: string[];\nexport async function f() {\n  for (const id of ids) {\n    await ofetch(\"/api/\" + id);\n  }\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
}

/// Exercises the class-method shape directly: the parser projects both a class symbol span and a nested
/// method sub-symbol span, overlapping. Without innermost-span priority this produces 2 findings; with it, 1.
#[test]
fn overlapping_class_and_method_spans_do_not_double_count() {
    let f = scan(
        "svc.ts",
        "declare const ids: string[];\nclass S {\n  httpService: any;\n  async f() {\n    for (const id of ids) {\n      await this.httpService.get(\"/u/\" + id);\n    }\n  }\n}\nexport { S };\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(snippet(&f[0]).contains("httpService.get"));
}

#[test]
fn comment_inside_the_function_body_documenting_a_loop_plus_fetch_example_is_not_flagged() {
    // A comment merely documenting a loop/fetch example (never actually executed) must not fire, even
    // though it textually satisfies both patterns within the span.
    let f = scan(
        "svc.ts",
        "declare const ids: string[];\nexport async function f() {\n  // Example: for (const id of ids) { fetch(url + id); } -- superseded by batchFetch below\n  return batchFetch(ids);\n}\ndeclare function batchFetch(ids: string[]): unknown;\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn api_in_loop_ok_marker_above_the_call_whitelists_it() {
    let f = scan(
        "svc.ts",
        "declare const ids: string[];\nexport async function f() {\n  for (const id of ids) {\n    // api-in-loop-ok: bounded admin list, sequential by design\n    await fetch(\"/api/\" + id);\n  }\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

// --- test-path exclusion: a mock endpoint hit in a loop in a test harness is not a production risk ---

#[test]
fn fetch_inside_loop_in_a_tests_directory_is_not_flagged() {
    let f = scan(
        "__tests__/svc.ts",
        "declare const ids: string[];\nexport async function f() {\n  for (const id of ids) {\n    const r = await fetch(\"/api/\" + id);\n    console.log(r);\n  }\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

// --- loop-token anchored to syntax, not a bare `\bdo\b`/`\bfor\b` word ---

#[test]
fn prose_string_literal_mentioning_do_is_not_a_loop() {
    // A bare `\bdo\b` word match would false-positive on ordinary prose like "logged in to do this".
    let f = scan(
        "svc.ts",
        "export async function f() {\n  const msg = \"You must be logged in to do this action\";\n  return fetch(\"/api/message\", { body: msg });\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn template_literal_containing_for_word_is_not_a_loop() {
    // A bare `\bfor\b` word match would false-positive on prose in a template literal like `for ${x} items`.
    let f = scan(
        "svc.ts",
        "declare const x: number;\nexport async function f() {\n  const msg = `waiting for ${x} items`;\n  return fetch(\"/api/status\", { body: msg });\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn real_for_loop_and_while_loop_and_do_while_still_flagged() {
    // The syntax-anchored loop pattern must still catch real `for (`/`while (`/`do {` constructs.
    let for_loop = scan(
        "svc.ts",
        "declare const ids: string[];\nexport async function f() {\n  for (let i = 0; i < ids.length; i++) {\n    await fetch(\"/api/\" + ids[i]);\n  }\n}\n",
    );
    assert_eq!(for_loop.len(), 1, "{for_loop:?}");

    let while_loop = scan(
        "svc2.ts",
        "declare const ids: string[];\nexport async function f() {\n  let i = 0;\n  while (i < ids.length) {\n    await fetch(\"/api/\" + ids[i]);\n    i++;\n  }\n}\n",
    );
    assert_eq!(while_loop.len(), 1, "{while_loop:?}");

    let do_while_loop = scan(
        "svc3.ts",
        "declare const ids: string[];\nexport async function f() {\n  let i = 0;\n  do {\n    await fetch(\"/api/\" + ids[i]);\n    i++;\n  } while (i < ids.length);\n}\n",
    );
    assert_eq!(do_while_loop.len(), 1, "{do_while_loop:?}");
}

// --- retry-shape veto ---

#[test]
fn fetch_in_a_bounded_retry_loop_is_not_flagged() {
    // A bounded retry loop around one call is not the N+1 this rule targets.
    let f = scan(
        "svc.ts",
        "export async function f(url: string) {\n  const maxRetries = 3;\n  for (let attempt = 0; attempt < maxRetries; attempt++) {\n    const r = await fetch(url);\n    if (r.ok) return r;\n  }\n  throw new Error(\"exhausted retries\");\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn fetch_in_a_for_of_retry_loop_mentioning_backoff_is_not_flagged() {
    // Same retry-guard veto, but on a for-of loop (rather than the traditional-for shape above) — the
    // veto is keyed off the function body mentioning retry/backoff vocabulary, not the loop's own shape.
    let f = scan(
        "svc.ts",
        "declare const urls: string[];\nexport async function f() {\n  const backoff = 100;\n  for (const url of urls) {\n    const r = await fetch(url);\n    console.log(r, backoff);\n  }\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

// --- structural span-based containment: trigger_in_loop (rewritten from text co-occurrence) ---

#[test]
fn fetch_inside_promise_all_map_callback_is_flagged() {
    let f = scan(
        "svc.ts",
        "declare const ids: string[];\ndeclare function url(id: string): string;\nexport async function f() {\n  await Promise.all(ids.map(async (id) => fetch(url(id))));\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 4);
    assert!(snippet(&f[0]).contains("fetch"));
}

#[test]
fn fetch_in_while_loop_condition_header_is_flagged() {
    // The loop-span header line is included by design (a call in the condition runs once per
    // iteration too), so a network call directly in a `while (...)` condition is in-span.
    let f = scan(
        "svc.ts",
        "declare const next: any;\nexport async function f() {\n  while (await fetch(next).then((r: any) => r.ok)) {\n    console.log(\"looping\");\n  }\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 3);
}

/// REDDIT-shape data adapter: one fetch, then the JSON response is TRANSFORMED via `.map()` over a
/// multi-line destructuring callback. The fetch line is not textually inside the map callback's span, so
/// the trigger never satisfies inside a loop span and the rule stays silent — the universal false-positive
/// shape (11/11 on the mono-hub corpus) this rewrite targets.
#[test]
fn single_fetch_then_response_array_map_transform_reddit_shape_is_not_flagged() {
    let f = scan(
        "svc.ts",
        "declare const url: string;\nexport async function f() {\n  const res = await fetch(url);\n  const json = await res.json();\n  return json.data.children.map(({ data }: any) => ({\n    id: data.id,\n    title: data.title,\n  }));\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

/// Stream-read shape: one fetch, then a `while` loop reads the response stream — the loop body mentions
/// `reader.read()`, not a network-pattern call, so the trigger pattern never matches inside the loop span
/// at all (and the one `fetch` call site itself sits outside every loop span).
#[test]
fn single_fetch_then_stream_read_while_loop_is_not_flagged() {
    let f = scan(
        "svc.ts",
        "declare const url: string;\ndeclare const reader: any;\nexport async function f() {\n  await fetch(url);\n  while (true) {\n    const { done, value } = await reader.read();\n    if (done) break;\n    console.log(value);\n  }\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

/// Regex-exec shape: one fetch, then a `while ((match = pattern.exec(xml)) !== null)` loop parses the
/// response text — same defect class as the stream-read shape above.
#[test]
fn single_fetch_then_regex_exec_while_loop_is_not_flagged() {
    let f = scan(
        "svc.ts",
        "declare const xml: string;\ndeclare const pattern: RegExp;\nexport async function f() {\n  const res = await fetch(\"/api/data\");\n  let match: RegExpExecArray | null;\n  while ((match = pattern.exec(xml)) !== null) {\n    console.log(match, res);\n  }\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");
}

/// BOUNDARY FINDING (documented deviation, not forced green): `(await fetch(url)).items.map(x => x.id)`
/// all on ONE line. Byte-wise the fetch is in the RECEIVER, outside the `.map()` callback's own span, so
/// this "should" be silent by the same logic as the multi-line REDDIT-shape negative above. But
/// `MethodScan::trigger_in_loop`'s containment check is LINE-based (`SourceFile::loop_spans` stores
/// 1-based line numbers, not byte offsets — see `extract_loop_spans`'s doc), and the one-liner's map
/// callback span is `(line, line)` for that single line. Since the fetch call and the callback share that
/// same line number, the trigger's line falls "within" the callback span and the rule FIRES — a real
/// false positive on the single-line receiver shape that the line-granularity containment check cannot
/// distinguish from genuine in-callback placement. Multi-line receiver shapes (see
/// `extract_loop_spans_map_callback_excludes_receiver_line` in parser-typescript) are unaffected because
/// the receiver's line differs from the callback's line range.
#[test]
fn single_line_receiver_fetch_before_map_callback_is_a_known_line_granularity_false_positive() {
    let f = scan(
        "svc.ts",
        "export async function f(url: string) {\n  return (await fetch(url)).items.map((x: any) => x.id);\n}\n",
    );
    assert_eq!(
        f.len(),
        1,
        "documented boundary finding: single-line receiver+callback share a line number under \
         line-granularity containment, so this fires instead of staying silent; if this assertion \
         ever flips to empty, the containment check has gained column/byte precision and this test \
         (and its doc comment) should be updated to assert silence instead — {f:?}"
    );
}
