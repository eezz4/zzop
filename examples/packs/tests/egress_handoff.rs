//! Where the localhost/private-IP addresses land once BOTH packs are loaded, held from both sides at once.
//!
//! ## What is actually being partitioned
//!
//! Two rules read the same addresses and ask different questions, which is why one of them can decline
//! without anyone being left holding a bag:
//!
//! * `egress/http-url-literal` (BUNDLED) asks whether a plain-`http://` literal could be a request over a
//!   public wire. Its `exclude_pattern` names `localhost`/`127.0.0.1`/`0.0.0.0`/`192.168.`/`10.x`/
//!   `172.16-31.x` outright, because none of those IS that wire — plain http to your own machine or your
//!   own network is not a downgrade. That exclusion is a statement about this rule's subject and holds
//!   whatever else is loaded.
//! * `code-hygiene/localhost-url-literal-committed` (THIS PACK) asks a different question about the same
//!   text: should that address have been config instead of a literal? Nothing in the line answers it —
//!   the same `http://localhost:3000` is broken in production and exactly right in a dev-only fixture —
//!   so the answer is a fact about the project, which is why the rule carries `axis: opinion` and ships
//!   opt-in rather than bundled.
//!
//! A default run therefore says nothing about a committed `http://localhost:3000`. That is not a gap left
//! by the export; it is what "we do not have an opinion about your project" looks like on the wire. Load
//! this pack to get the opinion. **The siblings' finding text does NOT carry that notice**, and that is a
//! decision rather than an omission: a finding reaches a user only on a line where it FIRES, while the
//! population that would need the notice is by construction the population where nothing fires. Hanging the
//! notice off a sibling delivers it to everyone except the people it is about. So the siblings' messages say
//! only what their own yardsticks measure, and the silence is documented where a reader can go looking for
//! it — this pack's `README.md`.
//!
//! ## Why the assertion needs both packs
//!
//! Under [`crate::scan`] the bundled sibling is not loaded at all, so "the sibling stayed silent" would
//! be true of a pack that does not exist — a vacuous assertion that cannot fail when the routing breaks.
//! [`crate::scan_both`] loads the bundled `egress` pack beside this one so the silence is a DECISION the
//! sibling made about a line it saw. `rules/dsl/egress/http_shapes.rs` keeps the mirror image, which is
//! what catches the failure from the other direction: a future widening of `http-url-literal` that
//! swallows the localhost case would make BOTH rules fire, and only a two-pack test can see it.

use crate::{egress_hits, hits, scan_both, TempDir};

/// The routing itself: one localhost literal, exactly one rule speaking about it, and it is this pack's.
#[test]
fn a_committed_localhost_literal_is_this_packs_and_the_bundled_sibling_declines_it() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/client.ts",
        "export const base = \"http://localhost:3000/api\";\n",
    );
    let out = scan_both(&dir);

    let mine = hits(&out, "localhost-url-literal-committed");
    assert_eq!(
        mine.len(),
        1,
        "the exported rule owns this line: {:?}",
        out.findings
    );
    assert_eq!(mine[0].line, 1);

    assert!(
        egress_hits(&out, "http-url-literal").is_empty(),
        "the bundled `http://` rule excludes localhost because loopback is not the public wire it \
         measures — if it starts firing here the two rules now double-report the same line: {:?}",
        out.findings
    );
}

/// The private-range half of the same exclude. `10.x`/`192.168.x`/`172.16-31.x` are the addresses
/// `http-url-literal`'s `exclude_pattern` lists beside `localhost`, so each one has to be shown landing
/// on this side of the split rather than in the gap between the two rules.
#[test]
fn every_private_range_the_bundled_rule_excludes_is_claimed_by_this_one() {
    for (name, url) in [
        ("loopback-v4", "http://127.0.0.1:5432/orders"),
        ("any-addr", "http://0.0.0.0:8080/health"),
        ("class-c", "http://192.168.1.10/api"),
        ("class-a", "http://10.0.4.12/v1/users"),
        ("class-b", "http://172.16.0.5/internal"),
    ] {
        let dir = TempDir::new("zzop-hygiene");
        dir.write(
            "src/client.ts",
            &format!("export const base = \"{url}\";\n"),
        );
        let out = scan_both(&dir);

        assert_eq!(
            hits(&out, "localhost-url-literal-committed").len(),
            1,
            "{name} ({url}) must be claimed by the exported rule: {:?}",
            out.findings
        );
        assert!(
            egress_hits(&out, "http-url-literal").is_empty(),
            "{name} ({url}) is inside the bundled rule's exclude and must stay there: {:?}",
            out.findings
        );
    }
}

/// The other side of the same boundary, so the pair is shown PARTITIONING rather than both declining.
/// A public `http://` host is the bundled rule's, and this pack must not have quietly widened into it.
#[test]
fn a_public_http_literal_stays_the_bundled_rules_and_this_pack_declines_it() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/client.ts",
        "export const base = \"http://example.com/api\";\n",
    );
    let out = scan_both(&dir);

    assert_eq!(
        egress_hits(&out, "http-url-literal").len(),
        1,
        "a public plain-http literal is the bundled rule's: {:?}",
        out.findings
    );
    assert!(
        hits(&out, "localhost-url-literal-committed").is_empty(),
        "{:?}",
        out.findings
    );
}
