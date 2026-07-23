//! MCP server over stdio: newline-delimited JSON-RPC 2.0. One loop, four method families —
//! `initialize`, `tools/*` (delegated to `tools`), `resources/*` (delegated to `resources`), plus the
//! JSON-RPC error surface. Silent-failure policy (the whole reason this module exists as more than a
//! `match`): every line that reaches us either gets a reply or is a spec-legal notification — a line we
//! cannot parse MUST NOT be swallowed (the client is left hanging on a reply that never comes; a Windows
//! path with unescaped backslashes, e.g. `"C:\Users\x"`, is invalid JSON and hit exactly this). Parse
//! failures answer with JSON-RPC `-32700` and `id: null` (the spec's reserved shape for "id was
//! unrecoverable"), non-object frames (including batch arrays, which this server does not support) with
//! `-32600` — both also log one line to stderr, the conventional MCP diagnostic channel.

use std::io::{BufRead, Write};

/// The version this binary reports as MCP `serverInfo.version` — re-exported from the shared
/// `zzop-host` crate (`CARGO_PKG_VERSION` there, the workspace `[workspace.package] version`, the
/// release SSOT since the 2026-07-22 version reform) so this server and the `zzop` CLI's `version`
/// subcommand can never disagree. CI verifies the pushed `v*` tag and `.claude-plugin/plugin.json`
/// both match it, so a released build's reported version equals the release tag and the plugin's
/// published version by construction.
pub use zzop_host::server::version;

/// MCP protocol versions this server actually supports, newest first. All three listed revisions
/// are genuinely supported, not aspirational: this server's surface (`initialize`, `tools/list`/
/// `tools/call` with text content, `resources/list`/`resources/read`) is semantically identical
/// across them — no revision-divergent feature (elicitation, structured tool output, auth) is
/// implemented. Listing the older revisions keeps older-SDK clients connectable where a
/// latest-only counter-offer could make them disconnect.
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
/// per JSON-RPC 2.0; an explicit `"id": null` is NOT a notification and is answered.
pub fn run_stdio() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let msg: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                // -32700 Parse error, id null: the request id is unrecoverable from a line that does
                // not parse, and the spec reserves exactly this response shape for that case.
                eprintln!("zzop-mcp: unparseable JSON-RPC line ({e})");
                respond(
                    &mut stdout,
                    serde_json::json!({
                        "jsonrpc": "2.0", "id": null,
                        "error": { "code": -32700, "message": format!("Parse error: {e}") }
                    }),
                );
                continue;
            }
        };
        if !msg.is_object() {
            // A JSON array here would be a JSON-RPC batch — unsupported by this server (and unused by
            // MCP clients). Saying so beats the previous behavior (falling into the "no id" branch and
            // silently never replying).
            eprintln!("zzop-mcp: non-object JSON-RPC frame (batch requests are not supported)");
            respond(
                &mut stdout,
                serde_json::json!({
                    "jsonrpc": "2.0", "id": null,
                    "error": { "code": -32600, "message": "Invalid Request: expected a single JSON-RPC object (batch arrays are not supported)" }
                }),
            );
            continue;
        }
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        // Notifications (no `id` key) are fire-and-forget — never reply.
        let Some(id) = msg.get("id").cloned() else {
            continue;
        };

        let reply = match method {
            "initialize" => {
                let requested = msg
                    .get("params")
                    .and_then(|p| p.get("protocolVersion"))
                    .and_then(|v| v.as_str());
                ok(
                    id,
                    serde_json::json!({
                        "protocolVersion": negotiate_protocol_version(requested),
                        "capabilities": { "tools": {}, "resources": {} },
                        "serverInfo": { "name": "zzop", "version": version() }
                    }),
                )
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
        };
        respond(&mut stdout, reply);
    }
}

fn ok(id: serde_json::Value, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn respond(stdout: &mut std::io::Stdout, value: serde_json::Value) {
    let _ = writeln!(stdout, "{value}");
    let _ = stdout.flush();
}

#[cfg(test)]
mod tests {
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
}
