//! MCP tool surface: definitions (`tools/list`) and dispatch (`tools/call`). This module is pure
//! protocol dispatch: extract the tool's arguments (`zzop_summary::args`), call the matching
//! `zzop_summary` function (config auto-discovery + facade call + summary assembly all live there —
//! see its crate doc), and wrap the result into the MCP reply shape. No shaping/filtering/warning-merge
//! logic lives here — if it did, it would be exactly the per-host drift the `zzop-summary` split exists
//! to prevent. The `zzop` CLI's twin subcommands (`analyze`/`cross`/`endpoint`/…) call the same
//! `zzop_summary` functions directly from `packages/cli-bin/src/main.rs` — there is no shared
//! per-product dispatch layer between the two, because a wrapper only one product traversed was drift
//! surface rather than a guard against it (2026-07-26 `crates/host` teardown).

mod definitions;
#[cfg(test)]
mod tests;

use zzop_summary::args;
use zzop_summary::FindingFilters;

pub use definitions::list;

/// `tools/call` dispatch. Tool-level failures return a normal MCP result with `isError: true` (the MCP
/// convention — protocol errors are only for malformed JSON-RPC, which `server` handles before us).
pub fn call(params: Option<&serde_json::Value>) -> serde_json::Value {
    let name = params
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let args = params.and_then(|p| p.get("arguments"));
    let outcome = match name {
        "analyze_repo" => (|| {
            // `path` XOR `configPath` — the same two source modes the `zzop analyze` CLI twin takes.
            // Both are OPTIONAL here and the shared handler decides: it owns "exactly one source", so
            // the two hosts cannot drift on which combinations are legal (a `required_string` here
            // would have made "neither" this layer's error and "both" the handler's).
            let path = args::optional_string(args, "path")?;
            let config_path = args::optional_string(args, "configPath")?;
            let filters = FindingFilters::from_args(args)?;
            zzop_summary::analyze_summary(path, config_path, &filters)
        })(),
        "cross_repo" => (|| {
            // Every declared-type violation (a non-array `paths`, a non-string element inside it, a
            // non-string `configPath`) is a named error here — see `zzop_summary::args`'s module doc
            // for the silent-fallback class this replaces.
            //
            // Source-mode exclusivity is NOT decided here, exactly as in `analyze_repo` above: both
            // sources are passed through and `zzop_config::trees::load_trees_request` owns "exactly one
            // source" for every host. This arm used to re-state "not both" / "pass one" locally, which
            // gave the MCP user one sentence and the CLI twin another for the same judgment — the
            // per-host drift the shared handler exists to prevent, reintroduced by the copy meant to
            // help. Pinned by `tests::cross_repo_source_mode_errors_are_the_shared_handlers_verbatim`.
            let paths = args::optional_string_array(args, "paths")?;
            let config_path = args::optional_string(args, "configPath")?;
            let filters = FindingFilters::from_args(args)?;
            zzop_summary::cross_summary(&paths, config_path, &filters)
        })(),
        "check_file" => (|| {
            let target = args::required_string(args, "target")?;
            let source_id = args::optional_string(args, "sourceId")?;
            let path = args::optional_string(args, "path")?;
            let paths = args::optional_string_array(args, "paths")?;
            let config_path = args::optional_string(args, "configPath")?;
            zzop_summary::file_summary(target, source_id, path, &paths, config_path)
        })(),
        "check_endpoint" => (|| {
            let pattern = args::required_string(args, "pattern")?;
            let path = args::optional_string(args, "path")?;
            let paths = args::optional_string_array(args, "paths")?;
            let config_path = args::optional_string(args, "configPath")?;
            zzop_summary::endpoint_summary(pattern, path, &paths, config_path)
        })(),
        "analyze_envelope" => (|| {
            let envelope_json = args::required_string(args, "envelopeJson")?;
            let filters = FindingFilters::from_args(args)?;
            zzop_summary::analyze_envelope_summary(envelope_json, &filters)
        })(),
        "check_coverage" => (|| {
            // Same source-mode shape as `cross_repo`, with ONE difference the handler owns: this lane
            // takes 1+ paths, not 2+, because "how much of this tree do you see" is a question about a
            // single tree first. `zzop_config::trees::resolve_trees_request` decides exclusivity for
            // both hosts here too — see the `cross_repo` arm for why this layer must not restate it.
            let paths = args::optional_string_array(args, "paths")?;
            let config_path = args::optional_string(args, "configPath")?;
            zzop_summary::coverage_summary(&paths, config_path)
        })(),
        "module_map" => (|| {
            // Same 1+ paths / configPath shape as `check_coverage`, and exclusivity is the shared
            // handler's call here too — see the `cross_repo` arm for why this layer must not restate
            // it. `fold` is REQUIRED on the wire while the `zzop map` CLI twin defaults it to 1: a
            // terminal caller is looking at the answer and can raise it, an agent is deciding how
            // much of the tree to pull into a context window and must say which grain it meant.
            let paths = args::optional_string_array(args, "paths")?;
            let config_path = args::optional_string(args, "configPath")?;
            let depth = args::optional_integer(args, "fold", 1)?
                .ok_or_else(|| "missing `fold` argument".to_string())?;
            zzop_summary::module_map(&paths, config_path, depth)
        })(),
        "validate_envelope" => args::required_string(args, "envelopeJson")
            .map(zzop_summary::validate_envelope_only_json),
        "validate_rule_pack" => {
            args::required_string(args, "packJson").map(zzop_summary::validate_rule_pack_json)
        }
        other => Err(format!("unknown tool: {other}")),
    };
    match outcome {
        Ok(text) => {
            // A `rule` filter that could not have matched anything is a FAILED call, not a clean one.
            // The `zzop analyze` twin exits 2 on the identical input; this lane answered `isError`-less
            // success with the reason 99% of the way into the document (measured: 28,757 bytes, offset
            // 28,406). The judgment is `zzop_summary`'s, read off the reply's structured roster — this
            // layer only decides WHICH tools have a `rule` argument, which is argument extraction and
            // therefore its own job. The join lane is excluded because the shared check says it must
            // be: its roster is per-`sources[]` and its findings carry native ids.
            let unmatchable = matches!(name, "analyze_repo" | "analyze_envelope")
                .then(|| args::optional_string(args, "rule").ok().flatten())
                .flatten()
                .and_then(|rule| zzop_summary::unmatchable_rule_filter(&text, rule));
            match unmatchable {
                // The reply stays `content[0]`, BYTE-IDENTICAL to what the CLI twin prints on stdout —
                // the surface-parity contract is about the document, and an exit code is not part of
                // it. `isError` is this host's exit code; the reason rides a SECOND block rather than
                // being spliced into the first, because a client that reads `content[0]` as JSON must
                // keep getting JSON.
                Some(reason) => serde_json::json!({
                    "content": [
                        { "type": "text", "text": text },
                        { "type": "text", "text": format!("zzop error: {reason}") },
                    ],
                    "isError": true
                }),
                None => serde_json::json!({ "content": [{ "type": "text", "text": text }] }),
            }
        }
        // This host appends its OWN spelling of the way out, at its own display layer — the mirror of
        // `cli/mod.rs`'s `Run \`zzop init\`` line, licensed by the same 2026-08-09 ruling that split the
        // shared refusal in half (`zzop_config::load`'s doc). The shared string names the ARTIFACT and
        // never a command, because `zzop init` is unactionable without a shell and `resources/read` is
        // unactionable in a terminal; each host supplies the half only it can act on.
        //
        // 🔴 Until 2026-09-10 this host supplied nothing HERE. Its half lived in `server::orientation`,
        // which is a different moment: orientation is paid every session and arrives before the agent
        // has any reason to read it, and whether a client forwards `initialize.instructions` to the
        // model is client-dependent. The refusal is paid only on failure and arrives exactly when it
        // is actionable. Measured (external review, `4f5d05d0`): the MCP refusal named the artifact
        // but carried the URI 0 times, so an agent had to already know that "contract document" means
        // an MCP resource and which of 17 URIs is meant.
        //
        // The URI is built from the shared table's prefix and name rather than spelled here, for the
        // reason `resources`'s module doc gives: a local copy is a pointer that can drift away from
        // the lane that has to answer it. `tools::tests` pins that `zzop init` still never rides this
        // wire — this line adds the MCP half, it does not relax that.
        Err(e) => {
            let text = if e.contains(zzop_summary::contracts::PATHS_MODE_CONFIG_MARKER) {
                // Third refusal of the same class. This host's word for "CONFIG MODE" is an argument
                // name, not a flag, so the shared sentence can name neither.
                format!("zzop error: {e}\nCall this tool again with `configPath` set to that config instead of `paths`.")
            } else if e.contains(zzop_summary::contracts::MULTI_TREE_MARKER) {
                // This host's half of the 2026-08-09 ruling for the multi-tree refusal: the shared
                // string names the join in prose, and the word for it here is a tool name, not a
                // shell line. Ordered first because the multi-tree message does not carry the
                // missing-config marker, so the two arms are disjoint either way.
                format!("zzop error: {e}\nCall the `cross_repo` tool with this config to analyze those trees together, or `analyze_repo` with a config that declares exactly one tree.")
            } else if e.contains(zzop_summary::contracts::MISSING_CONFIG_MARKER) {
                format!(
                    "zzop error: {e}\nRead {}{} from this server (`resources/read`) and save those bytes as that file.",
                    zzop_summary::contracts::URI_PREFIX,
                    zzop_summary::contracts::CONFIG_TEMPLATE_NAME,
                )
            } else {
                format!("zzop error: {e}")
            };
            serde_json::json!({
                "content": [{ "type": "text", "text": text }],
                "isError": true
            })
        }
    }
}
