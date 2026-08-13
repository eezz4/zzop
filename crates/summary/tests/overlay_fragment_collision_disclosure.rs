//! G11's DELIVERY — the fragment-collision disclosure reaches the published `warnings` field of a
//! real reply, on a real tree, through the real config surface an adapter author actually uses.
//!
//! # Why this exists, and why it is not in `crates/engine`
//! The engine cannot give the router-composition fragment channels a collision policy: `overrides`
//! covers `imports` only, a fragment has no key the merge could compare on, and scoping roots per
//! producer would forbid the very cross-producer mount Mode B advertises. That reasoning is owned by
//! `crates/engine/src/envelope/merge/collisions.rs`'s module doc and is not restated here. What
//! matters for THIS file is the consequence: zzop answers a capability gap with a DISCLOSURE. Once
//! that trade is made, the disclosure ARRIVING is the entire justification for it — a remedy the user
//! never reads is the same output as no remedy at all.
//!
//! What existed before this file covered the FIRST leg only, and measurably so. Deleting the whole
//! disclosure fails exactly two targets in the workspace: `crates/engine/src/envelope/tests/overlay.rs`'s
//! `an_overlay_describing_routers_another_producer_already_described_is_disclosed` (a unit test on
//! `apply_adapter_overlays`' own `&mut Vec<String>`) and this one. Deleting only the REMEDY — the two
//! sentences that carry the entire "disclose instead of implement" judgment — failed exactly one:
//! this one. That unit test asserts the line exists and names its file, fragment and producer; it
//! asserts nothing about the remedy, nothing about the sibling `procedure-router` channel, and by
//! construction nothing about delivery. Its subject is a `Vec<String>` an engine function was handed.
//!
//! Between that vector and a user sit `zzop_facade::analyze_json`'s output view and
//! `summary::analyze::shape::shape_analyze_output`, either of which could drop, rename or re-shape the
//! field with every existing test still green. This repo has already been bitten by exactly that gap
//! (a ⚠ notice that existed in the code and reached no run), so a disclosure that substitutes for a
//! capability gets its pin at the LAST layer, not only the first.
//!
//! So this file sits in `crates/summary/tests/`: `analyze_summary` is the one function every host
//! (`zzop analyze`, MCP `analyze_repo`) calls, its return value IS the published reply, and its
//! `warnings` array IS the wire field. Reaching it from here also exercises the config lane —
//! `overlays: [...]` in a `zzop.config.jsonc`, resolved by `zzop_config`'s mapper into
//! `adapterOverlays` — which is how an adapter author attaches an overlay in the first place. A test
//! calling `zzop_engine::analyze_tree` directly would prove none of that chain.
//!
//! # What is asserted, and what is deliberately NOT
//! Properties, never the sentence. A pin that reproduces the message verbatim goes red on every
//! rewording, which trains people to update the expectation instead of reading it. The three
//! properties below are the ones that make this line a disclosure rather than a notification:
//!   1. it is THERE, in `warnings`, on a run that collides;
//!   2. it NAMES THE COLLIDING FILES — `reports.rs` states the rule it lives by, "a number nobody can
//!      trace back is not a disclosure", and a count alone leaves the author with no file to edit;
//!   3. it carries a PRESCRIPTION — the user's own `files[]`, or emitting only the channels the other
//!      producer left empty. This is the whole content of the "disclose instead of implement"
//!      judgment; strip it and the line becomes an apology.
//!
//! Both by-name fragment channels are covered in one run (`router-mount` and `procedure-router`), so
//! the sibling that shares the same missing policy cannot be disclosed for one and silent for the
//! other. `class_shape_fragments` is out of scope by design — it has its own conflict policy
//! (poison + disclose), see `record_fragment_collisions`' doc.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

/// The substring that identifies the G11 line among the run's warnings. Kept to the fragment-channel
/// noun phrase rather than the whole opening clause: this is a locator, not one of the assertions.
const COLLISION_MARKER: &str = "already carried router-composition fragments";

fn default_filters() -> zzop_summary::FindingFilters {
    zzop_summary::FindingFilters::new(None, None, None).expect("no-filter view always constructs")
}

/// A self-cleaning temp tree (std-only; this crate's tests share no test-utils module — same pattern
/// as `disclosure_fold.rs` and `crates/engine/tests/integration/analyze_adapter_overlay.rs`).
struct TempTree(PathBuf);

impl TempTree {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "zzop-overlay-collision-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp tree must be creatable");
        TempTree(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("temp subdir must be creatable");
        }
        fs::write(full, content).expect("temp file must be writable");
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const ROUTER_FILE: &str = "src/routes.ts";
const TRPC_FILE: &str = "src/trpc.ts";

/// Two NATIVELY-parsed files, one per by-name fragment channel: a Hono-style router mount
/// (`router_mount_fragments`) and a tRPC router (`procedure_router_fragments`). The overlay below
/// describes these same two paths, which is precisely the "two producers describe one file" shape —
/// and it is also the MISUSE the recipe warns about (pointing an adapter at a tree the native parser
/// already reads), which is why the engine's answer is a disclosure rather than a merge policy.
fn tree_with_native_fragments(name: &str) -> TempTree {
    let dir = TempTree::new(name);
    dir.write(
        ROUTER_FILE,
        "export const app = new Hono()\n  .get('/native/ping', pingHandler);\n",
    );
    dir.write(
        TRPC_FILE,
        "export const appRouter = router({\n  ping: publicProcedure.query(() => 1),\n});\n",
    );
    dir
}

/// An overlay describing BOTH native files' fragment channels. Written as an on-disk JSON document
/// and attached through `zzop.config.jsonc`'s `overlays` key — the same two artifacts an adapter
/// author writes — rather than constructed as a Rust value, so the config->request->engine lane is
/// part of what this pins.
fn attach_colliding_overlay(dir: &TempTree) {
    let overlay = json!({
        "format": "zzop-normalized-ast",
        // A released contract version at or below this build's own is accepted forever (see
        // `zzop_core::normalized::validate`), and nothing here needs a versioned channel — so this
        // fixture does not have to move when the current version does.
        "version": "0.27.0",
        "parser": "collision-fixture-adapter/1",
        "source": "adapter",
        "files": [
            {
                "path": ROUTER_FILE,
                "loc": 2,
                "router_mount_fragments": [{
                    "name": "overlayApp",
                    "entries": [{"Verb": {
                        "method": "GET",
                        "path": "/overlay/ping",
                        "handler": null,
                        "line": 1
                    }}]
                }]
            },
            {
                "path": TRPC_FILE,
                "loc": 3,
                "procedure_router_fragments": [{
                    "name": "overlayRouter",
                    "entries": [{"Leaf": {"key": "ping", "verb": "QUERY", "line": 1}}]
                }]
            }
        ]
    });
    dir.write("overlay.json", &overlay.to_string());
    dir.write(
        "zzop.config.jsonc",
        r#"{ "roots": ["."], "overlays": ["overlay.json"] }"#,
    );
}

/// The published reply's `warnings` array, from the entry point every host shares.
fn warnings_of(dir: &TempTree) -> Vec<String> {
    let out = zzop_summary::analyze_summary(
        Some(&dir.path().display().to_string()),
        None,
        &default_filters(),
    )
    .expect("analyze must succeed on this tree");
    let reply: serde_json::Value = serde_json::from_str(&out).expect("a reply is JSON");
    reply["warnings"]
        .as_array()
        .unwrap_or_else(|| panic!("the reply must carry a `warnings` array: {out}"))
        .iter()
        .map(|w| w.as_str().expect("every warning is a string").to_string())
        .collect()
}

/// THE PIN. All three properties on one colliding run.
#[test]
fn a_colliding_overlay_puts_its_disclosure_and_its_remedy_on_the_published_warnings_field() {
    let dir = tree_with_native_fragments("pin");
    attach_colliding_overlay(&dir);
    let warnings = warnings_of(&dir);

    // (1) It arrives. Not "the engine collected it" — it is in the field a host prints.
    let line = warnings
        .iter()
        .find(|w| w.contains(COLLISION_MARKER))
        .unwrap_or_else(|| {
            panic!(
                "the reply must disclose that two producers describe the same file — the engine \
                 answers this capability gap with a disclosure, so a disclosure that does not reach \
                 `warnings` leaves the gap answered by nothing at all. warnings: {warnings:#?}"
            )
        });

    // (2) It names the colliding files. `reports.rs`: "a number nobody can trace back is not a
    // disclosure" — an author given only a count has no file to drop from `files[]`.
    for path in [ROUTER_FILE, TRPC_FILE] {
        assert!(
            line.contains(path),
            "the disclosure must name the colliding file, not just count it — {path} is missing \
             from: {line}"
        );
    }

    // Both by-name fragment channels, in the one disclosure. `procedure_router_fragments` shares the
    // missing collision policy with `router_mount_fragments`; a line that covered only the latter
    // would leave the sibling silent for the identical defect.
    for channel in ["router-mount", "procedure-router"] {
        assert!(
            line.contains(channel),
            "the disclosure must name the channel that collided ({channel}) — both by-name fragment \
             channels share this policy gap: {line}"
        );
    }

    // (3) It prescribes. Asserted as PROPERTIES (the surface the author edits, and the narrower
    // alternative), never as the sentence: a pin on the exact wording goes red on every rewrite and
    // teaches people to re-record it instead of read it. What must survive any rewrite is that the
    // line points at the OVERLAY'S OWN declaration — the engine has no remedy to offer, which is
    // exactly why this disclosure exists instead of a merge policy.
    assert!(
        line.contains("files[]"),
        "the disclosure must point at the overlay's own `files[]` — without a remedy naming what the \
         author controls, choosing disclosure over capability buys nothing: {line}"
    );
    assert!(
        line.contains("channels"),
        "and offer the narrower alternative (emit only the channels the other producer left empty), \
         which is the second half of the remedy: {line}"
    );
}

/// The control half. Without the overlay the same tree is silent, so the assertions above are
/// measuring the collision rather than a line every run happens to carry.
#[test]
fn the_same_tree_without_the_overlay_says_nothing_about_fragment_collisions() {
    let dir = tree_with_native_fragments("control");
    dir.write("zzop.config.jsonc", r#"{ "roots": ["."] }"#);
    let warnings = warnings_of(&dir);
    assert!(
        !warnings.iter().any(|w| w.contains(COLLISION_MARKER)),
        "one producer is not a collision: {warnings:#?}"
    );
}
