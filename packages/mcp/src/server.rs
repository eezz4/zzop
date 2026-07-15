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
                let proto = msg
                    .get("params")
                    .and_then(|p| p.get("protocolVersion"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("2025-06-18");
                ok(
                    id,
                    serde_json::json!({
                        "protocolVersion": proto,
                        "capabilities": { "tools": {}, "resources": {} },
                        "serverInfo": { "name": "zzop", "version": env!("CARGO_PKG_VERSION") }
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
