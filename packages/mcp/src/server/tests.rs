//! Protocol tests for the stdio JSON-RPC loop.
//!
//! Why a table and not a bespoke test per case: before this file the entire dispatch (`run_stdio`) was
//! entered by no Rust test at all, while `tests/server_bin.rs` claimed the opposite ("covered by the
//! protocol unit tests in `server.rs`" — those tests called `version()` and
//! `negotiate_protocol_version()`, neither of which reaches the dispatch). A claim of coverage that
//! does not exist is worse than a gap: it postpones verification indefinitely.
//!
//! Precisely (not "zero coverage", which would be too strong): `scripts/measure/snapshot.mjs` does
//! drive this loop, and `scripts/measure/detection-gate.sh` runs it. But it spawns a process per TOOL
//! CALL and feeds each one the same two-line happy path — `initialize`, then one `tools/call`. Every
//! error lane (parse failure, unknown method, non-object frame), the notification no-reply contract,
//! and any sequence other than that fixed pair are outside what it can observe, no matter how many
//! snapshots run; and it needs a built binary plus a corpus, so it is not reachable from `cargo test`.
//!
//! The table shape is the point: each remaining protocol defect becomes one row rather than a new test.

use super::{handle_line, handle_message, serve};
use serde_json::{json, Value};

/// One row = one line arriving on the wire, and what the server must answer for it.
struct Case {
    /// What this row pins (printed on failure).
    what: &'static str,
    /// The exact line the client writes.
    line: &'static str,
    /// The reply the server must produce. `None` means "must produce NO reply at all" — the
    /// notification contract, which an assertion over reply *contents* can never express.
    check: fn(Option<&Value>),
}

fn err_code(reply: Option<&Value>) -> i64 {
    reply.expect("a reply is required")["error"]["code"]
        .as_i64()
        .expect("error.code must be an integer")
}

const CASES: &[Case] = &[
    Case {
        what: "initialize echoes a supported requested version and names the server",
        line: r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#,
        check: |reply| {
            let r = reply.expect("initialize must be answered");
            assert_eq!(r["jsonrpc"], "2.0");
            assert_eq!(r["id"], 1);
            assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
            assert_eq!(r["result"]["serverInfo"]["name"], "zzop");
            assert_eq!(r["result"]["serverInfo"]["version"], super::version());
            assert!(r["result"]["capabilities"]["tools"].is_object());
            assert!(r["result"]["capabilities"]["resources"].is_object());
        },
    },
    Case {
        what: "initialize counter-offers the latest supported version for an unsupported request",
        line: r#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"9999-99-99"}}"#,
        check: |reply| {
            let r = reply.expect("initialize must be answered");
            assert_eq!(
                r["result"]["protocolVersion"],
                super::LATEST_PROTOCOL_VERSION
            );
        },
    },
    Case {
        what: "initialize with no params still negotiates (no panic on a missing params object)",
        line: r#"{"jsonrpc":"2.0","id":3,"method":"initialize"}"#,
        check: |reply| {
            let r = reply.expect("initialize must be answered");
            assert_eq!(
                r["result"]["protocolVersion"],
                super::LATEST_PROTOCOL_VERSION
            );
        },
    },
    Case {
        what: "tools/list returns the tool table under result.tools",
        line: r#"{"jsonrpc":"2.0","id":4,"method":"tools/list"}"#,
        check: |reply| {
            let r = reply.expect("tools/list must be answered");
            assert_eq!(r["id"], 4);
            let tools = r["result"]["tools"]
                .as_array()
                .expect("result.tools must be an array");
            assert!(!tools.is_empty(), "the server advertises at least one tool");
            assert!(
                tools.iter().all(|t| t["name"].is_string()),
                "every advertised tool is named"
            );
        },
    },
    Case {
        what: "tools/call on an unknown tool is a RESULT with isError, not a JSON-RPC error",
        line: r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"no_such_tool","arguments":{}}}"#,
        check: |reply| {
            let r = reply.expect("tools/call must be answered");
            assert!(
                r.get("error").is_none(),
                "tool-level failures use the MCP isError convention, not the protocol error lane"
            );
            assert_eq!(r["result"]["isError"], true);
            let text = r["result"]["content"][0]["text"]
                .as_str()
                .expect("error text");
            assert!(text.contains("no_such_tool"), "got: {text}");
        },
    },
    Case {
        what: "tools/call reaches a real handler (argument error surfaces as isError text)",
        line: r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"validate_rule_pack","arguments":{}}}"#,
        check: |reply| {
            let r = reply.expect("tools/call must be answered");
            assert_eq!(r["result"]["isError"], true);
            let text = r["result"]["content"][0]["text"]
                .as_str()
                .expect("error text");
            assert!(text.contains("packJson"), "got: {text}");
        },
    },
    Case {
        what: "resources/list returns the embedded contract table",
        line: r#"{"jsonrpc":"2.0","id":7,"method":"resources/list"}"#,
        check: |reply| {
            let r = reply.expect("resources/list must be answered");
            let resources = r["result"]["resources"]
                .as_array()
                .expect("result.resources must be an array");
            assert!(!resources.is_empty());
        },
    },
    Case {
        what: "resources/read on an unknown uri is -32602 (invalid params), not a silent drop",
        line: r#"{"jsonrpc":"2.0","id":8,"method":"resources/read","params":{"uri":"zzop://contract/nope"}}"#,
        check: |reply| {
            assert_eq!(err_code(reply), -32602);
            assert_eq!(reply.unwrap()["id"], 8);
        },
    },
    Case {
        what: "an unknown method is -32601 and names the method it did not find",
        line: r#"{"jsonrpc":"2.0","id":9,"method":"prompts/list"}"#,
        check: |reply| {
            assert_eq!(err_code(reply), -32601);
            let message = reply.unwrap()["error"]["message"].as_str().unwrap();
            assert!(message.contains("prompts/list"), "got: {message}");
        },
    },
    Case {
        // The regression this whole file exists for: a Windows path with unescaped backslashes is
        // invalid JSON, and a swallowed parse failure leaves the client waiting forever on a reply.
        what: "unparseable JSON is answered -32700 with id null, never swallowed",
        line: r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"path":"C:\Users\x"}}"#,
        check: |reply| {
            assert_eq!(err_code(reply), -32700);
            assert_eq!(
                reply.unwrap()["id"],
                Value::Null,
                "the id is unrecoverable from a line that does not parse"
            );
        },
    },
    Case {
        // THIS ROW REPLACES the one that pinned `-32600` for every array ("a non-object frame (a batch
        // array) is -32600, not a silent drop"), which described the contract up to 2026-07-29. That
        // contract contradicted `2025-03-26` — one of the three revisions `SUPPORTED_PROTOCOL_VERSIONS`
        // advertises — whose basic/index says servers MUST support receiving batches. The row is not
        // deleted; it is rewritten to the new contract, so the change could not land silently.
        what: "a batch array is answered with an ARRAY of the individual replies",
        line: r#"[{"jsonrpc":"2.0","id":11,"method":"tools/list"}]"#,
        check: |reply| {
            let replies = reply
                .expect("a batch carrying a request must be answered")
                .as_array()
                .expect("the reply to a batch is an array");
            assert_eq!(
                replies.len(),
                1,
                "one request in, one reply element out: {replies:?}"
            );
            assert_eq!(replies[0]["id"], 11);
            assert!(replies[0]["result"]["tools"].is_array());
        },
    },
    Case {
        // Order is the one thing the JSON-RPC spec leaves to the server ("the Server MAY process a
        // batch ... in any order"). This server processes strictly left to right and emits in that same
        // order — the cheapest thing a single-threaded loop can do, and the only choice a client that
        // (wrongly) matches positionally instead of by id also survives. Pinned so it stays a decision.
        what: "batch replies come back in request order, one element per request",
        line: r#"[{"jsonrpc":"2.0","id":21,"method":"tools/list"},{"jsonrpc":"2.0","id":22,"method":"resources/list"},{"jsonrpc":"2.0","id":23,"method":"prompts/list"}]"#,
        check: |reply| {
            let replies = reply
                .expect("a batch carrying requests must be answered")
                .as_array()
                .expect("the reply to a batch is an array");
            assert_eq!(
                replies.len(),
                3,
                "three requests, three replies: {replies:?}"
            );
            assert_eq!(replies[0]["id"], 21);
            assert_eq!(replies[1]["id"], 22);
            assert_eq!(replies[2]["id"], 23);
            assert!(replies[0]["result"]["tools"].is_array());
            assert!(replies[1]["result"]["resources"].is_array());
            // A per-element failure is per-element: the unknown method answers -32601 without
            // disturbing its neighbours.
            assert_eq!(replies[2]["error"]["code"], -32601);
        },
    },
    Case {
        what: "a notification inside a batch contributes NO element to the reply array",
        line: r#"[{"jsonrpc":"2.0","method":"notifications/initialized"},{"jsonrpc":"2.0","id":31,"method":"tools/list"}]"#,
        check: |reply| {
            let replies = reply
                .expect("the request in the batch must still be answered")
                .as_array()
                .expect("the reply to a batch is an array");
            assert_eq!(
                replies.len(),
                1,
                "the notification adds nothing to the array: {replies:?}"
            );
            assert_eq!(replies[0]["id"], 31);
        },
    },
    Case {
        // The case implementations usually get wrong: `[]` would be a REPLY to something that asked for
        // none, on a channel the client is parsing positionally. The spec says emit nothing at all.
        what: "a batch of ONLY notifications produces no reply at all, not an empty array",
        line: r#"[{"jsonrpc":"2.0","method":"notifications/initialized"},{"jsonrpc":"2.0","method":"notifications/cancelled"}]"#,
        check: |reply| {
            assert!(
                reply.is_none(),
                "a batch with nothing to answer is answered with nothing, got: {reply:?}"
            );
        },
    },
    Case {
        // An empty array is not a batch of zero — the spec calls it an invalid request outright, and
        // the answer is ONE error object, not an array containing one.
        what: "an EMPTY array is itself an invalid request: a single -32600 object, not an array",
        line: "[]",
        check: |reply| {
            assert_eq!(err_code(reply), -32600);
            assert_eq!(reply.unwrap()["id"], Value::Null);
            assert!(
                !reply.unwrap().is_array(),
                "the answer to `[]` is a bare error object, not a one-element batch reply"
            );
        },
    },
    Case {
        what:
            "an invalid element inside a batch is an error object IN the array; the batch survives",
        line: r#"[1,{"jsonrpc":"2.0","id":41,"method":"tools/list"},"not a request"]"#,
        check: |reply| {
            let replies = reply
                .expect("a batch with one valid request must be answered")
                .as_array()
                .expect("the reply to a batch is an array");
            assert_eq!(
                replies.len(),
                3,
                "each element answers for itself — one bad element does not fail the batch: {replies:?}"
            );
            assert_eq!(replies[0]["error"]["code"], -32600);
            assert_eq!(replies[0]["id"], Value::Null);
            assert_eq!(replies[1]["id"], 41);
            assert!(replies[1]["result"]["tools"].is_array());
            assert_eq!(replies[2]["error"]["code"], -32600);
            assert_eq!(replies[2]["id"], Value::Null);
        },
    },
    Case {
        // The spec's own "rpc call with invalid Batch" example is exactly this shape. Before
        // 2026-07-29 an OBJECT with no string `method` fell through to the no-id branch and was
        // answered with SILENCE — treated as a notification purely because it was too malformed to
        // recognize — while three prose sites promised -32600. This row is the one that would have
        // caught that: the scalar rows above cannot, because they never reach the object branch.
        what: "an OBJECT element that is not a valid request is -32600, not silence",
        line: r#"[{"foo":"bar"},{"jsonrpc":"2.0","method":1,"params":"bar"}]"#,
        check: |reply| {
            let replies = reply
                .expect(
                    "a batch of malformed OBJECTS must still answer — silence here is the defect",
                )
                .as_array()
                .expect("the reply to a batch is an array");
            assert_eq!(
                replies.len(),
                2,
                "every element answers for itself: {replies:?}"
            );
            for r in replies {
                assert_eq!(r["error"]["code"], -32600, "not a request object: {r:?}");
                assert_eq!(r["id"], Value::Null);
            }
        },
    },
    Case {
        what: "a top-level scalar (neither an object nor a batch) is still -32600",
        line: "42",
        check: |reply| {
            assert_eq!(err_code(reply), -32600);
            assert_eq!(reply.unwrap()["id"], Value::Null);
        },
    },
    Case {
        what: "a notification (no id) gets NO reply, per JSON-RPC 2.0",
        line: r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        check: |reply| assert!(reply.is_none(), "a notification must not be answered"),
    },
    Case {
        what: "a notification naming a KNOWN method is still not answered",
        line: r#"{"jsonrpc":"2.0","method":"tools/list"}"#,
        check: |reply| assert!(reply.is_none(), "no id means no reply, whatever the method"),
    },
    Case {
        what: "an explicit `id: null` is NOT a notification and IS answered",
        line: r#"{"jsonrpc":"2.0","id":null,"method":"tools/list"}"#,
        check: |reply| {
            let r = reply.expect("an explicit null id is a request");
            assert_eq!(r["id"], Value::Null);
            assert!(r["result"]["tools"].is_array());
        },
    },
    Case {
        what: "a blank line is skipped without a reply",
        line: "   ",
        check: |reply| assert!(reply.is_none(), "blank lines produce nothing"),
    },
];

/// Every reply this server emits is a JSON-RPC 2.0 envelope with exactly one of result/error.
fn assert_envelope(r: &Value, what: &str) {
    assert_eq!(r["jsonrpc"], "2.0", "[{what}] missing jsonrpc marker");
    assert!(
        r.get("id").is_some(),
        "[{what}] every reply carries an id key"
    );
    assert_ne!(
        r.get("result").is_some(),
        r.get("error").is_some(),
        "[{what}] a reply carries exactly one of result/error"
    );
}

#[test]
fn protocol_table() {
    for case in CASES {
        let reply = handle_line(case.line);
        if let Some(r) = &reply {
            match r.as_array() {
                // A batch reply is an array of exactly those envelopes, so the same check applies
                // element-wise — and it is never empty, because a batch with nothing to answer is
                // answered with nothing rather than with `[]`.
                Some(elements) => {
                    assert!(
                        !elements.is_empty(),
                        "[{}] an empty array is never put on the wire",
                        case.what
                    );
                    for element in elements {
                        assert_envelope(element, case.what);
                    }
                }
                None => assert_envelope(r, case.what),
            }
        }
        (case.check)(reply.as_ref());
    }
}

/// `handle_message` is the seam batch receiving maps over an array: it takes an already-parsed value,
/// so a batch element and a single top-level object take the identical path — which is what makes
/// "an element of a batch is answered exactly as it would be alone" true by construction rather than
/// by a second implementation kept in sync.
#[test]
fn handle_message_is_the_parsed_value_seam_handle_line_delegates_to() {
    let line = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
    let parsed: Value = serde_json::from_str(line).unwrap();
    assert_eq!(handle_message(&parsed), handle_line(line));
    // And the no-reply contract survives the seam.
    let notification: Value = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    assert!(handle_message(&notification).is_none());
}

/// The staleness self-report's WIRING, pinned without reading the clock: whatever
/// `staleness::notice()` decides for this build, `initialize` must carry exactly that — the string
/// under `instructions` when there is a notice, and NO `instructions` key at all when there is not.
/// Asserting a concrete presence/absence instead would make this test's meaning depend on when it runs
/// (a fresh checkout is silent today and would speak in a year), which is how a conditional surface
/// ends up untested in both directions. The two directions of the DECISION are tested where the
/// decision is made, over injected inputs: `crate::staleness::tests`.
///
/// An emitted `instructions` must be a plain string — the field's spec type. A `Value::String` wrapper
/// is easy to lose to a `json!` macro that stringifies a struct instead.
#[test]
fn initialize_always_orients_and_appends_the_staleness_notice_when_there_is_one() {
    let reply = handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
        .expect("initialize must be answered");
    let instructions = reply["result"]["instructions"]
        .as_str()
        .expect("`instructions` must always be present and a plain string");
    // The three things an agent otherwise learns by FAILING. Measured 2026-08-07: this slot was
    // staleness-only and absent in the current build, so a client's whole briefing was seven tool
    // descriptions — none of which can say "a config must exist before any of us runs".
    assert!(instructions.contains("zzop.config.jsonc"), "{instructions}");
    assert!(instructions.contains("config-template"), "{instructions}");
    assert!(instructions.contains("cross_repo"), "{instructions}");
    // Whatever `staleness::notice()` decides for this build must still ride, appended. Asserting a
    // concrete presence/absence would make this test's meaning depend on WHEN it runs (a fresh
    // checkout is silent today and would speak in a year); the decision's two directions are tested
    // over injected inputs in `crate::staleness::tests`.
    if let Some(notice) = crate::staleness::notice() {
        assert!(
            instructions.contains(&notice),
            "the staleness notice must survive alongside the orientation: {instructions}"
        );
    }
}

/// Drives the loop over ONE connection carrying several lines, in-process. The measurement harness
/// gets exactly one shape here (`initialize` then one `tools/call`); everything below varies what the
/// second and third lines are, which is where the loop-level defects live.
fn transcript(input: &str) -> Vec<Value> {
    let mut output: Vec<u8> = Vec::new();
    serve(std::io::BufReader::new(input.as_bytes()), &mut output);
    String::from_utf8(output)
        .expect("replies are UTF-8")
        .lines()
        .map(|l| serde_json::from_str(l).expect("every emitted line is JSON"))
        .collect()
}

#[test]
fn two_sequential_requests_on_one_connection_are_both_answered_in_order() {
    let replies = transcript(concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        "\n",
    ));
    assert_eq!(replies.len(), 2, "one reply per request: {replies:?}");
    assert_eq!(replies[0]["id"], 1);
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(replies[1]["id"], 2);
    assert!(replies[1]["result"]["tools"].is_array());
}

#[test]
fn a_notification_between_two_requests_shifts_no_reply_onto_the_wrong_request() {
    // The failure this pins: if the notification produced a reply (or the loop mis-tracked ids), the
    // client would read reply #2 as the answer to its second request. Nothing outside this file sends
    // a notification to this server at all, so nothing else can see this class.
    let replies = transcript(concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\n",
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"resources/list"}"#,
        "\n",
    ));
    assert_eq!(
        replies.len(),
        2,
        "the notification and the blank line are silent: {replies:?}"
    );
    assert_eq!(replies[0]["id"], 1);
    assert_eq!(replies[1]["id"], 2);
    assert!(replies[1]["result"]["resources"].is_array());
}

#[test]
fn a_malformed_line_does_not_end_the_connection() {
    // The silent-failure policy in one assertion: a bad line is ANSWERED (-32700) and the connection
    // keeps serving. A loop that bailed on the parse error would leave the client hung on request #2.
    let replies = transcript(concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        "\n",
        r#"{"broken"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        "\n",
    ));
    assert_eq!(
        replies.len(),
        3,
        "three lines in, three replies out: {replies:?}"
    );
    assert_eq!(replies[0]["id"], 1);
    assert_eq!(replies[1]["error"]["code"], -32700);
    assert_eq!(replies[1]["id"], Value::Null);
    assert_eq!(replies[2]["id"], 2, "the connection survived the bad line");
}

#[test]
fn a_batch_line_emits_exactly_one_wire_line_carrying_the_reply_array() {
    // Loop-level half of the batch contract: N requests arrive on ONE line and N replies leave on ONE
    // line. A loop that wrote one line per element would desynchronise a client reading line-by-line.
    let replies = transcript(concat!(
        r#"[{"jsonrpc":"2.0","id":1,"method":"tools/list"},{"jsonrpc":"2.0","id":2,"method":"resources/list"}]"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/list"}"#,
        "\n",
    ));
    assert_eq!(
        replies.len(),
        2,
        "two lines in, two lines out — the batch is one line: {replies:?}"
    );
    let batch = replies[0].as_array().expect("the batch line is an array");
    assert_eq!(batch.len(), 2);
    assert_eq!(batch[0]["id"], 1);
    assert_eq!(batch[1]["id"], 2);
    assert_eq!(replies[1]["id"], 3, "the single request is NOT wrapped");
}

#[test]
fn a_notification_only_batch_puts_nothing_on_the_wire_and_the_connection_survives() {
    let replies = transcript(concat!(
        r#"[{"jsonrpc":"2.0","method":"notifications/initialized"}]"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        "\n",
    ));
    assert_eq!(
        replies.len(),
        1,
        "the notification-only batch is silent — not even `[]`: {replies:?}"
    );
    assert_eq!(replies[0]["id"], 1);
}

#[test]
fn a_last_line_without_a_trailing_newline_is_still_served() {
    let replies = transcript(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0]["id"], 1);
}

// `version()` reports `CARGO_PKG_VERSION` = the workspace version (release SSOT since the 2026-07-22
// version reform — no `ZZOP_RELEASE_VERSION` env). CI verifies the release tag matches it.
#[test]
fn version_reports_cargo_pkg_version() {
    assert_eq!(super::version(), env!("CARGO_PKG_VERSION"));
}

#[test]
fn negotiate_echoes_a_supported_requested_protocol_version() {
    // Every listed revision echoes — an older-SDK client (2024-11-05) keeps its own version
    // rather than being counter-offered into a disconnect.
    for v in super::SUPPORTED_PROTOCOL_VERSIONS {
        assert_eq!(super::negotiate_protocol_version(Some(v)), *v);
    }
}

#[test]
fn negotiate_counter_offers_latest_supported_for_unsupported_or_missing_versions() {
    // An unsupported request must NOT be echoed back (that would falsely claim support) —
    // the spec's answer is the server's latest supported version.
    assert_eq!(
        super::negotiate_protocol_version(Some("9999-99-99")),
        super::LATEST_PROTOCOL_VERSION
    );
    assert_eq!(
        super::negotiate_protocol_version(None),
        super::LATEST_PROTOCOL_VERSION
    );
    // Sanity: the counter-offer is itself a supported version.
    assert!(super::SUPPORTED_PROTOCOL_VERSIONS.contains(&super::LATEST_PROTOCOL_VERSION));
}
