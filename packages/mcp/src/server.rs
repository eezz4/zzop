//! MCP server over stdio: newline-delimited JSON-RPC 2.0. One loop, four method families —
//! `initialize`, `tools/*` (delegated to `tools`), `resources/*` (delegated to `resources`), plus the
//! JSON-RPC error surface. Silent-failure policy (the whole reason this module exists as more than a
//! `match`): every line that reaches us either gets a reply or is a spec-legal notification — a line we
//! cannot parse MUST NOT be swallowed (the client is left hanging on a reply that never comes; a Windows
//! path with unescaped backslashes, e.g. `"C:\Users\x"`, is invalid JSON and hit exactly this). Parse
//! failures answer with JSON-RPC `-32700` and `id: null` (the spec's reserved shape for "id was
//! unrecoverable"), frames that are neither a request object nor a batch array with `-32600` — both
//! also log one line to stderr, the conventional MCP diagnostic channel.
//!
//! A JSON array IS a batch and IS served (`handle_batch`): receiving batches is a MUST of the
//! `2025-03-26` revision this server advertises, so refusing them (the behavior up to 2026-07-29) made
//! that advertisement false. Batches are only RECEIVED — nothing here originates requests, so there is
//! no sending side to batch.

use std::io::{BufRead, Write};

/// The version this binary reports as MCP `serverInfo.version` — re-exported from the shared
/// `zzop-summary` crate, whose own re-export reaches the single owner `zzop_facade::version`
/// (`CARGO_PKG_VERSION` there, the workspace `[workspace.package] version`, the release SSOT since the
/// 2026-07-22 version reform) so this server and the `zzop` CLI's `version` subcommand can never
/// disagree. CI verifies the pushed `v*` tag and `.claude-plugin/plugin.json`
/// both match it, so a released build's reported version equals the release tag and the plugin's
/// published version by construction.
pub use zzop_summary::version;

/// The DIAGNOSTIC version form this binary prints for `zzop-mcp version --verbose`: the same release
/// version plus every parser's derived fingerprint (`<id>/<source hash>`) and the engine's own
/// (`zzop-engine=<hash>`). Re-exported from the same shared crate the bare
/// form comes from, reaching the same single owner (`zzop_facade::version_string`), and byte-identical
/// to what `zzop version --verbose` prints — the parity half of that CLI knob, so "which parser
/// build read these files?" is answerable from either product rather than the CLI alone. (Build, not
/// just frontend, since 2026-08-03: each value carries the source hash that keys the per-file cache,
/// so it moves whenever extraction code moves — see `zzop_facade::version_string`.)
///
/// It is NOT on the MCP wire: `serverInfo` is a spec-shaped `{name, version}` object and every
/// `resources/read` document this server serves is a static embedded contract, so a runtime fingerprint
/// string would have to invent a new resource class to ride there. The operator question it answers
/// ("which build is this?") is already answered on this binary's own stderr banner and this subcommand.
pub use zzop_summary::version_string;

/// MCP protocol versions this server actually supports, newest first. All three listed revisions
/// are genuinely supported, not aspirational: this server's surface (`initialize`, `tools/list`/
/// `tools/call` with text content, `resources/list`/`resources/read`) is semantically identical
/// across them — no revision-divergent feature (elicitation, structured tool output, auth) is
/// implemented. Listing the older revisions keeps older-SDK clients connectable where a
/// latest-only counter-offer could make them disconnect.
///
/// The one requirement that does NOT hold uniformly across the three is JSON-RPC batching: the middle
/// entry, `2025-03-26`, says a server MUST support RECEIVING batches; `2025-06-18` deleted that and
/// `2024-11-05` never had it. Up to 2026-07-29 this file answered every array `-32600`, so the "not
/// aspirational" above was itself aspirational for one of the three revisions it vouched for.
/// `handle_batch` closes that: the claim is now true for each entry in this list, which is the only
/// condition under which an entry belongs in it.
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// The server's latest supported protocol version — the spec-mandated counter-offer when a client
/// requests a version this server does not support.
const LATEST_PROTOCOL_VERSION: &str = "2025-06-18";

/// MCP version negotiation, per spec: reply with the client's requested version only when this
/// server supports it; otherwise reply with the server's latest supported version. Echoing an
/// arbitrary requested version verbatim (the previous behavior) falsely claims support for e.g.
/// "9999-99-99" — the client is entitled to treat the echoed version's semantics as honored.
/// A missing/non-string `protocolVersion` param also gets the latest supported version.
fn negotiate_protocol_version(requested: Option<&str>) -> &'static str {
    SUPPORTED_PROTOCOL_VERSIONS
        .iter()
        .copied()
        .find(|supported| Some(*supported) == requested)
        .unwrap_or(LATEST_PROTOCOL_VERSION)
}

/// Runs the stdio server until stdin closes. Notifications (parsed objects with no `id`) get no reply,
/// per JSON-RPC 2.0 — nor does a batch whose elements are all notifications; an explicit `"id": null`
/// is NOT a notification and is answered.
pub fn run_stdio() {
    serve(std::io::stdin().lock(), &mut std::io::stdout());
}

/// The transport loop, over any reader/writer so the protocol is testable ON ONE CONNECTION without
/// spawning anything. That is the whole reason this is not inlined into `run_stdio`: the only other
/// driver of this loop (`scripts/measure/snapshot.mjs`, via `detection-gate.sh`) spawns a process per
/// tool call and feeds each the same two-line happy path, so it cannot vary the sequence.
fn serve(input: impl BufRead, output: &mut impl Write) {
    for line in input.lines() {
        let Ok(line) = line else { break };
        if let Some(reply) = handle_line(&line) {
            let _ = writeln!(output, "{reply}");
            let _ = output.flush();
        }
    }
}

/// One wire line in, at most one reply LINE out — a batch's individual replies travel together as one
/// array on that single line, so the line-per-line rhythm a client reads by never changes. `None` means
/// "emit nothing", which is a real protocol state (a blank line, a JSON-RPC notification, or a batch of
/// only notifications) and not an error — collapsing it into an empty reply would put an unrequested
/// line on a channel the client is parsing positionally.
pub fn handle_line(line: &str) -> Option<serde_json::Value> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let msg: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            // -32700 Parse error, id null: the request id is unrecoverable from a line that does
            // not parse, and the spec reserves exactly this response shape for that case.
            log_protocol_error(format_args!("unparseable JSON-RPC line ({e})"));
            return Some(serde_json::json!({
                "jsonrpc": "2.0", "id": null,
                "error": { "code": -32700, "message": format!("Parse error: {e}") }
            }));
        }
    };
    match &msg {
        serde_json::Value::Array(elements) => handle_batch(elements),
        _ => handle_message(&msg),
    }
}

/// Dispatches a JSON-RPC 2.0 BATCH: an array of requests/notifications, answered with an array of the
/// individual replies. Every element goes through the same `handle_message` a lone top-level object
/// does, so "an element is answered exactly as it would be alone" holds by construction rather than by
/// a second implementation kept in sync.
///
/// Three shapes the spec calls out, and they are the ones implementations get wrong:
/// - a notification element contributes NO element, so a batch of only notifications is answered with
///   NOTHING — never `[]`, which would be an unrequested line on a channel read positionally.
/// - an EMPTY array is not a batch of zero: the spec calls it an invalid request outright, answered
///   with ONE bare `-32600` object rather than an array containing one.
/// - an element that is not a valid request answers `-32600` IN the array, leaving its neighbours
///   alone; one bad element does not fail the batch.
///
/// ORDER: elements are processed left to right and the replies keep that order. The spec lets a server
/// answer in any order (the client matches by id), so this is a free choice — it is the only thing a
/// single-threaded loop can do without buffering, and it additionally keeps working the client that
/// (wrongly) matches positionally. Pinned by a table row so it stays a decision rather than an accident.
fn handle_batch(elements: &[serde_json::Value]) -> Option<serde_json::Value> {
    if elements.is_empty() {
        log_protocol_error(format_args!("empty JSON-RPC batch array"));
        return Some(invalid_request(
            "Invalid Request: an empty array is not a valid JSON-RPC batch",
        ));
    }
    let replies: Vec<serde_json::Value> = elements.iter().filter_map(handle_message).collect();
    if replies.is_empty() {
        // Every element was a notification: nothing to answer means answer nothing, NOT `[]`.
        return None;
    }
    Some(serde_json::Value::Array(replies))
}

/// The ONE stderr writer in this package's library, and the reason the four protocol-error branches
/// above call a function instead of writing the line themselves. It also owns the `zzop-mcp: ` prefix,
/// which was four separate string literals before.
///
/// Why this library prints at all, when `clippy::print_stderr` says a library must not: this module IS
/// the stdio transport. Its stdout is the JSON-RPC wire — one reply object per line, read positionally
/// — so a diagnostic there would corrupt the protocol, which is exactly what `print_stdout` (a warn in
/// the workspace lint table, with no exemption here) prevents. Stderr is the conventional MCP
/// diagnostic channel and the only channel left. The alternative, staying silent, is the
/// silent-failure this module's header names as the class it exists to close: a malformed frame
/// answered on the wire but invisible to the operator debugging why their client sent it.
///
/// Takes `fmt::Arguments` rather than `&str` so call sites keep `eprintln!`-style formatting without
/// allocating a `String` per diagnostic.
#[allow(
    clippy::print_stderr,
    reason = "See this function's doc: stdout carries the JSON-RPC wire, so stderr is the only \
              diagnostic channel this transport has. Deliberately the only site in this library that \
              is exempt — the exemption is one function wide, not one file and not one crate."
)]
fn log_protocol_error(detail: std::fmt::Arguments<'_>) {
    eprintln!("zzop-mcp: {detail}");
}

/// The JSON-RPC `-32600` reply, always with `id: null`: a frame that is not a well-formed request
/// object is one whose id cannot be trusted to exist, and the spec reserves this shape for that.
fn invalid_request(message: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": null,
        "error": { "code": -32600, "message": message }
    })
}

/// Dispatches one already-parsed JSON-RPC message. Split from the transport (`serve`) and from
/// parsing (`handle_line`) so the protocol is a pure function of the message — which is what makes the
/// error lanes and the notification contract testable as a table rather than as spawned processes.
///
/// ONE branch is not pure and is named here rather than left for a reader to discover: `initialize`
/// reads the wall clock through `crate::staleness::notice()`. The impurity is confined to that call,
/// the value it produces is tested through `staleness::notice_at` (both inputs are arguments there), and
/// the WIRING is pinned clock-independently by `tests::initialize_carries_the_staleness_notice_iff_there_is_one`.
pub fn handle_message(msg: &serde_json::Value) -> Option<serde_json::Value> {
    if !msg.is_object() {
        // Two ways to land here: a top-level frame that is neither an object nor a batch array (a bare
        // scalar), or a batch element that is not a request object — including a nested array, since
        // JSON-RPC has no nested batches. Both are `-32600`; inside a batch this is that ELEMENT's
        // reply, not the batch's. Answering at all beats the pre-2026-07-27 behavior (falling into the
        // "no id" branch and silently never replying).
        log_protocol_error(format_args!("JSON-RPC frame is not a request object"));
        return Some(invalid_request(
            "Invalid Request: expected a JSON-RPC request object",
        ));
    }
    // An object with no STRING `method` is not a request at all — JSON-RPC 2.0 section 5 answers it
    // `-32600`, and the spec's own "invalid Batch" example uses exactly this shape
    // (`{"jsonrpc":"2.0","method":1,"params":"bar"}`). Checked BEFORE the id, deliberately: reading the
    // id first meant `{"foo":"bar"}` took the no-id branch and was answered with silence, i.e. treated
    // as a notification purely because it was too malformed to be recognized. A one-element batch of
    // that shape produced no wire line at all. That is the silent-swallow this module's header names as
    // the class it exists to close, and three prose sites (twice here, once in docs/modules/mcp.md)
    // already promised the `-32600` — the code, not the prose, was the thing that was wrong.
    let Some(method) = msg.get("method").and_then(|m| m.as_str()) else {
        log_protocol_error(format_args!("JSON-RPC object carries no string `method`"));
        return Some(invalid_request(
            "Invalid Request: a request object must carry a string \"method\"",
        ));
    };
    // Notifications (a well-formed request with no `id` key) are fire-and-forget — never reply.
    let id = msg.get("id").cloned()?;

    Some(match method {
        "initialize" => {
            let requested = msg
                .get("params")
                .and_then(|p| p.get("protocolVersion"))
                .and_then(|v| v.as_str());
            let mut result = serde_json::json!({
                "protocolVersion": negotiate_protocol_version(requested),
                "capabilities": { "tools": {}, "resources": {} },
                "serverInfo": { "name": "zzop", "version": version() }
            });
            // The staleness self-report rides `instructions`, and ONLY when there is one to make (see
            // `crate::staleness` for all four silences). `instructions` is an optional string on
            // `InitializeResult` in every revision this server advertises, and its defined purpose —
            // text the host may pass to the model, like a system prompt — is exactly the reach this
            // needs: the `.mcpb` lane has no delivery-layer notifier, so the notice has to arrive
            // somewhere a reader actually looks. `serverInfo` could not carry it (it is a spec-shaped
            // `{name, version}` object), and stderr alone is a log file nobody opens unprompted.
            //
            // Since 2026-08-08 the slot ALWAYS carries orientation, with the staleness notice
            // appended when there is one. Before that it was staleness-only, and measured absent in
            // the shipped build — so a client's entire briefing was seven tool descriptions. Those
            // descriptions are this repo's most honest surface (each names what it cannot do first),
            // but none of them can state a fact that is true BEFORE any tool runs: that a config must
            // exist at all. The agent learned it by failing, which is the one thing this server can
            // cheaply prevent.
            let mut instructions = orientation::ORIENTATION.to_string();
            if let Some(notice) = crate::staleness::notice() {
                instructions.push_str("\n\n");
                instructions.push_str(&notice);
            }
            result["instructions"] = serde_json::Value::String(instructions);
            ok(id, result)
        }
        "tools/list" => ok(id, crate::tools::list()),
        "tools/call" => ok(id, crate::tools::call(msg.get("params"))),
        "resources/list" => ok(id, crate::resources::list()),
        "resources/read" => match crate::resources::read(msg.get("params")) {
            Ok(result) => ok(id, result),
            Err(e) => serde_json::json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32602, "message": e }
            }),
        },
        _ => serde_json::json!({
            "jsonrpc": "2.0", "id": id,
            "error": { "code": -32601, "message": format!("method not found: {method}") }
        }),
    })
}

fn ok(id: serde_json::Value, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

mod orientation;

#[cfg(test)]
mod tests;
