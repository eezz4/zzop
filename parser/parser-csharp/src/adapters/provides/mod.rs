//! ASP.NET Core HTTP route PROVIDES extraction — two independent, same-file idioms (task brief, mirrors
//! `zzop_parser_java_21::provides`'s single-idiom Spring shape, extended to two producers the way
//! `zzop_parser_go::adapters`' `net_http`+`gin` combination works):
//! - `attribute_controller` — the dominant idiom: `[ApiController]`/`[Controller]`-attributed (or
//!   `*Controller`-named) classes, `[HttpGet]`/`[HttpPost]`/.../`[Route]` method attributes, an optional
//!   class-level `[Route("api/[controller]")]` base path with `[controller]`-token substitution.
//! - `minimal_api` — `app.MapGet("/x", ...)`-style top-level route registrations, including a chained
//!   `app.MapGroup("/api").MapGet("/x", ...)` group prefix.
//!
//! Both producers share ONE parse (crate root doc's "parse once per public fn call" — two sibling
//! PRODUCERS sharing the same public call's parse is not a second parse, unlike two different PUBLIC
//! FNS each parsing independently, the same accounting `zzop_parser_go::adapters::
//! extract_go_router_fragments`'s doc pins for its own `net_http`+`gin` pair). A file exercising BOTH
//! idioms emits both producers' provides concatenated (attribute-controller routes first, then
//! minimal-API routes) with no cross-producer reconciliation — same "rare pattern, document rather than
//! engineer around" tradeoff `zzop_parser_go::adapters`'s own module doc accepts for `net/http`+`gin`.

pub(crate) mod attribute_controller;
pub(crate) mod minimal_api;

use zzop_core::IoProvide;

/// Extracts every ASP.NET Core HTTP route PROVIDE from one C# file's raw source — see module doc for
/// the two recognized idioms. Never panics: empty on parse failure.
pub fn extract_csharp_http_provides(rel: &str, text: &str) -> Vec<IoProvide> {
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    attribute_controller::extract(rel, tree.root_node(), text, &mut out);
    minimal_api::extract(rel, tree.root_node(), text, &mut out);
    out
}

#[cfg(test)]
mod tests;
