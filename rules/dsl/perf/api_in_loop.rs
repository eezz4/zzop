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
        "declare const ids: string[];\nexport async function f() {\n  for (const id of ids) {\n    // zzop-api-in-loop-ok: bounded admin list, sequential by design\n    await fetch(\"/api/\" + id);\n  }\n}\n",
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
    // MULTILINE callback on purpose: a single-line `ids.map(async (id) => fetch(url(id)))` gets NO
    // loop span anymore (single-line callback spans prove nothing line-granularly — see the boundary
    // test at the bottom of this file), so the per-iteration claim is only made where the callback
    // body has lines of its own.
    let f = scan(
        "svc.ts",
        "declare const ids: string[];\ndeclare function url(id: string): string;\nexport async function f() {\n  await Promise.all(ids.map(async (id) => {\n    return fetch(url(id));\n  }));\n}\n",
    );
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].line, 5);
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
/// shape this rewrite targets.
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

/// The single-line callback boundary, now SILENT from the producer: `(await fetch(url)).items.map(x =>
/// x.id)` all on ONE line. Byte-wise the fetch is in the RECEIVER, outside the `.map()` callback's own
/// span — but the containment channel is LINE-based, so a `(line, line)` callback span could not be told
/// apart from the one-shot receiver call sharing that line, and this shape used to FIRE (it was pinned
/// here as a documented line-granularity false positive). The parser now drops single-line callback
/// spans entirely (`SourceFile::loop_spans`'s doc owns the rule), so both one-line shapes below are
/// silent: the receiver-fetch one (a false positive retired) and the network-call-around-the-map one
/// (the review-reproduced `client.get(ids.map(...).join(...))`). Cost, published as intended
/// under-reporting: a GENUINE per-iteration network call in a one-line callback (`ids.map(async (id) =>
/// fetch(url(id)))` on one line) is also lost — the multiline positive
/// `fetch_inside_promise_all_map_callback_is_flagged` is the pair proving real ones still fire.
#[test]
fn single_line_map_callback_shapes_are_silent_because_the_span_is_not_emitted() {
    let f = scan(
        "svc.ts",
        "export async function f(url: string) {\n  return (await fetch(url)).items.map((x: any) => x.id);\n}\n",
    );
    assert!(f.is_empty(), "{f:?}");

    let g = scan(
        "svc2.ts",
        "declare const client: any;\ndeclare const ids: string[];\nexport async function f() {\n  return await client.get(ids.map((i) => i).join(','));\n}\n",
    );
    assert!(g.is_empty(), "{g:?}");
}
