//! The per-rule resource space — `zzop://rule/<id>`, the parameterised half of `resources/*`.
//!
//! Split out of `resources.rs` when that file crossed the 400-line cap (2026-09-13). The cut is the
//! seam the file already had rather than a line-count trim: the sibling owns the CONTRACT table (a
//! fixed roster of embedded documents, listed in full), and this owns a space that is parameterised
//! and never listed. Two different answers to "what can be read", and only one of them enumerates.

/// The per-rule URI space: `zzop://rule/<id>` resolves to that rule's full text — the same bytes
/// `zzop explain <id>` prints, from the same function.
///
/// ## Why this exists (2026-09-13, review ledger V163 / the reply-size work)
///
/// The product decision is that a reply carries what differs per finding and the FIXED explanation
/// has one owner, `zzop explain` (`output-philosophy.md` §3.5). That decision named a CLI
/// subcommand, and an MCP agent cannot run one. Measured before this landed: the eight tools carried
/// no `explain`, and the only channel to a rule's full text was the whole `rule-catalog` contract at
/// **210,076 bytes** — 1.6x the reply that motivated shortening replies in the first place. Sending
/// an agent there to avoid 33k tokens costs it 52k.
///
/// So the order in `now.md` is channel FIRST, body removal second: shortening `message` before this
/// exists leaves the agent in a room with no door, which is the honesty violation the same file's
/// step 3 is written to prevent.
///
/// ## Why a resource TEMPLATE and not a tool or 178 resources
///
/// A ninth tool would add its schema to `tools/list`, and that list is a fixed cost every session
/// pays before asking anything (measured 32,127 bytes). Spending it to shrink replies is the wrong
/// direction. Listing all rule ids as individual resources moves the same bloat to `resources/list`.
/// A template is one entry that parameterises, which is the shape MCP has for exactly this, and it
/// is a base feature of all three revisions this server advertises.
pub(super) const RULE_URI_PREFIX: &str = "zzop://rule/";

/// `resources/templates/list` — the parameterised half of the resource surface.
///
/// Advertised unconditionally, like `resources/list`: this server's `initialize` declares a
/// `resources` capability, and templates are part of it in every revision here. Until this landed the
/// method fell through to `-32601`, which tells a conforming client the server has no templates
/// rather than that it was never asked.
pub fn templates_list() -> serde_json::Value {
    serde_json::json!({
        "resourceTemplates": [{
            "uriTemplate": format!("{RULE_URI_PREFIX}{{id}}"),
            "name": "rule",
            "title": "One rule's full text",
            "description":
                "The complete text for ONE rule id — what it detects, what it cannot see, how to \
                 suppress it, and how to turn it off. Identical bytes to `zzop explain <id>`. \
                 `id` is a DSL rule id as a finding carries it in `ruleId` — `<pack>/<rule>`, e.g. \
                 `security/hardcoded-secret` — so a reader that has such a finding never has to guess \
                 one. A NATIVE ANALYSIS id (bare, or `cross-layer/`- or `schema/`-prefixed) is \
                 REFUSED here: this lookup only reads the compiled-in DSL pack data, and a native \
                 analysis's full prose entry is in the `zzop://contract/rule-catalog` resource \
                 instead. Read this when a reply's message is folded or shortened and you need the \
                 rule's own account of its limits.",
            "mimeType": "text/plain",
        }]
    })
}

/// Resolve `zzop://rule/<id>`. Returns `None` when the uri is not in this space, so the caller can
/// fall through to the contract space rather than this function guessing which error to write.
pub(super) fn read_rule(uri: &str) -> Option<Result<serde_json::Value, String>> {
    let id = uri.strip_prefix(RULE_URI_PREFIX)?;
    if id.is_empty() {
        return Some(Err(format!(
            "resource uri {uri:?} names no rule id. The shape is `{RULE_URI_PREFIX}<id>`, and the id \
             is the one a finding carries in `ruleId`."
        )));
    }
    // `zzop_summary::explain` is the SAME function `zzop explain <id>` calls, re-exported for this
    // reason -- one lookup, so the terminal and the wire cannot drift about what a rule says. Its
    // `Err` is already a caller-facing sentence naming the lookup lane that failed, so it is passed
    // through rather than rewritten: a second wording here would be a second owner.
    Some(match zzop_summary::explain(id) {
        Ok(text) => Ok(serde_json::json!({
            "contents": [{ "uri": uri, "mimeType": "text/plain", "text": text }]
        })),
        Err(message) => Err(message),
    })
}
