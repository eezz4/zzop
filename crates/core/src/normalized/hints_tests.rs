//! Tests for the advisory hint pass (`normalized::hints`) and its separation from the validity axis.

use super::*;
use crate::io::{IoConsume, IoProvide};

fn provide(kind: &str, key: &str, file: &str, line: u32) -> IoProvide {
    IoProvide {
        kind: kind.to_string(),
        key: key.to_string(),
        file: file.to_string(),
        line,
        symbol: None,
        body: None,
    }
}

fn consume(kind: &str, key: Option<&str>, file: &str, line: u32) -> IoConsume {
    IoConsume {
        kind: kind.to_string(),
        key: key.map(str::to_string),
        file: file.to_string(),
        line,
        raw: None,
        method: None,
        body: None,
        client: None,
        retry_configured: None,
    }
}

/// A structurally VALID envelope (correct format/version, non-empty unique paths) carrying whatever
/// `io`/paths a test wants to hint on — so every hint assertion below is also an assertion that the
/// envelope itself was acceptable.
fn envelope(files: Vec<FileProjection>) -> NormalizedEnvelope {
    NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: SUPPORTED_NORMALIZED_AST_VERSION,
        parser: "hint-fixture/1".to_string(),
        source: "s".to_string(),
        files,
    }
}

fn file(path: &str, io: IoFacts) -> FileProjection {
    FileProjection {
        path: path.to_string(),
        loc: 1,
        io,
        ..Default::default()
    }
}

fn io(provides: Vec<IoProvide>, consumes: Vec<IoConsume>) -> IoFacts {
    IoFacts { provides, consumes }
}

// --- 1. absolute `files[].path` ------------------------------------------------------------------

/// Seals: an absolute `files[].path` — POSIX, Windows drive, or UNC — is hinted, because an envelope
/// carries tree-relative paths: every path-keyed surface reports the producer's machine layout, and as
/// a Mode B overlay the path matches no artifact in the tree at all.
#[test]
fn absolute_file_paths_are_hinted_on_every_platform_spelling() {
    for path in [
        "/srv/app/src/user.ts",
        "C:\\work\\app\\src\\user.ts",
        "c:/work/app/src/user.ts",
        "\\\\build-share\\app\\src\\user.ts",
    ] {
        let hints = envelope_hints(&envelope(vec![file(path, IoFacts::default())]));
        assert_eq!(hints.len(), 1, "expected one hint for {path}: {hints:?}");
        assert!(hints[0].contains("absolute path"), "{hints:?}");
    }
}

/// Seals the no-violation side: ordinary tree-relative paths (including `./`-prefixed and
/// deeply-nested ones) draw no hint — the check never fires on the shape adapters are told to emit.
#[test]
fn relative_file_paths_draw_no_hint() {
    for path in [
        "src/user.ts",
        "./src/user.ts",
        "webapp/legacy/user.jsp",
        "a.ext",
    ] {
        let hints = envelope_hints(&envelope(vec![file(path, IoFacts::default())]));
        assert!(hints.is_empty(), "unexpected hint for {path}: {hints:?}");
    }
}

// --- 2. non-normalized `http` keys ---------------------------------------------------------------

/// Seals: an `http` key that the core keying helper would not have produced is hinted, on BOTH sides,
/// and the hint names the exact canonical key to emit instead (the round-trip, not a shape regex — a
/// lowercase verb, a missing leading slash and an unsubstituted `:id` all pass `^[A-Z]+ /`-style
/// thinking on some spelling but miss the exact-key join).
#[test]
fn non_normalized_http_keys_are_hinted_with_the_canonical_form() {
    let cases = [
        ("get /users", "GET /users"),
        ("GET users", "GET /users"),
        ("GET /users/:id", "GET /users/{}"),
        ("GET //users//", "GET /users"),
        ("/users", "no method/path split"), // nothing to canonicalize — the message says so instead
    ];
    for (key, expected_fragment) in cases {
        let hints = envelope_hints(&envelope(vec![file(
            "src/a.ts",
            io(vec![provide("http", key, "src/a.ts", 3)], vec![]),
        )]));
        assert_eq!(hints.len(), 1, "expected one hint for {key}: {hints:?}");
        assert!(hints[0].contains(expected_fragment), "{hints:?}");
    }

    // Consume side normalizes differently — a query suffix is dropped there (and only there), which is
    // the shape that silently misses every join.
    let hints = envelope_hints(&envelope(vec![file(
        "src/a.ts",
        io(
            vec![],
            vec![consume(
                "http",
                Some("GET /articles?limit=10"),
                "src/a.ts",
                9,
            )],
        ),
    )]));
    assert_eq!(hints.len(), 1, "{hints:?}");
    assert!(hints[0].contains("GET /articles'"), "{hints:?}");
}

/// Seals the no-violation side plus the never-guess gate: canonical keys (including the `?`
/// unknown-verb sentinel and a provide-side `?` wildcard pattern), non-`http` kinds with keys that
/// look nothing like `"METHOD /path"`, and an unresolved consume (`key: null`) all draw no hint.
#[test]
fn canonical_keys_other_kinds_and_unresolved_consumes_draw_no_hint() {
    let hints = envelope_hints(&envelope(vec![file(
        "src/a.ts",
        io(
            vec![
                provide("http", "GET /users/{}", "src/a.ts", 3),
                provide("http", "? /pages/{}", "src/a.ts", 4),
                provide("http", "GET /files/a?b", "src/a.ts", 5),
                provide("db-table", "table:users", "src/a.ts", 6),
                provide("queue", "orders.created", "src/a.ts", 7),
            ],
            vec![
                consume("http", Some("POST /users"), "src/a.ts", 10),
                consume("http", None, "src/a.ts", 11),
                consume("db-table", Some("table:users"), "src/a.ts", 12),
            ],
        ),
    )]));
    assert!(hints.is_empty(), "{hints:?}");
}

// --- 3. a host in a PROVIDE key ------------------------------------------------------------------

/// Seals: a provide key carrying `://` is hinted — a provide keys this tree's own interface path, and
/// a host-carrying key is bucketed as third-party egress on the consume side, so nothing can resolve
/// to it. Reported once, as the host diagnosis, not also as a normal-form miss.
#[test]
fn a_provide_key_carrying_a_host_is_hinted_once() {
    let hints = envelope_hints(&envelope(vec![file(
        "src/a.ts",
        io(
            vec![provide(
                "http",
                "GET https://api.example.com/users",
                "src/a.ts",
                3,
            )],
            vec![],
        ),
    )]));
    assert_eq!(hints.len(), 1, "{hints:?}");
    assert!(hints[0].contains("carries a host"), "{hints:?}");
}

/// Seals the no-violation side, which is the asymmetry itself: a CONSUME key may carry a host — that
/// is the documented external-egress shape (`crate::io`'s egress gate) — so neither the host check nor
/// the normal-form round-trip may fire on it.
#[test]
fn a_consume_key_carrying_a_host_is_legitimate_egress_and_draws_no_hint() {
    let hints = envelope_hints(&envelope(vec![file(
        "src/a.ts",
        io(
            vec![],
            vec![consume(
                "http",
                Some("GET https://api.stripe.com/v1/charges"),
                "src/a.ts",
                9,
            )],
        ),
    )]));
    assert!(hints.is_empty(), "{hints:?}");
}

// --- 4. duplicate provide ------------------------------------------------------------------------

/// Seals: two provides with the identical `(kind, key, file, line)` are hinted once (the second
/// emission), tree-wide rather than per-file — an adapter that emits a route twice usually does it
/// from two passes over different files.
#[test]
fn a_duplicate_provide_at_the_identical_location_is_hinted() {
    let hints = envelope_hints(&envelope(vec![
        file(
            "src/a.ts",
            io(vec![provide("http", "GET /users", "src/a.ts", 3)], vec![]),
        ),
        file(
            "src/b.ts",
            io(vec![provide("http", "GET /users", "src/a.ts", 3)], vec![]),
        ),
    ]));
    assert_eq!(hints.len(), 1, "{hints:?}");
    assert!(hints[0].contains("duplicate provide"), "{hints:?}");
}

/// Seals the no-violation side: the identity is all four of `(kind, key, file, line)`, so the same key
/// provided at a different line/file, or a different kind at the same line, is a legal multi-provider
/// emission and draws nothing.
#[test]
fn provides_differing_in_any_identity_component_draw_no_hint() {
    let hints = envelope_hints(&envelope(vec![file(
        "src/a.ts",
        io(
            vec![
                provide("http", "GET /users", "src/a.ts", 3),
                provide("http", "GET /users", "src/a.ts", 9),
                provide("http", "GET /users", "src/b.ts", 3),
                provide("db-table", "table:users", "src/a.ts", 3),
            ],
            vec![],
        ),
    )]));
    assert!(hints.is_empty(), "{hints:?}");
}

// --- the contract: hints never decide validity ---------------------------------------------------

/// THE PIN for this feature: an envelope that trips all four hints is still VALID. Hints ride beside
/// the verdict; promoting one to a rejection would break the frozen v1 contract (and the CLI exit
/// code) for an envelope that conforms to it.
#[test]
fn an_envelope_tripping_every_hint_is_still_valid() {
    let hint_bait = envelope(vec![file(
        "/srv/app/src/a.ts",
        io(
            vec![
                provide("http", "get /users", "src/a.ts", 3),
                provide("http", "GET https://api.example.com/x", "src/a.ts", 4),
                provide("http", "GET /ok", "src/a.ts", 5),
                provide("http", "GET /ok", "src/a.ts", 5),
            ],
            vec![],
        ),
    )]);
    let json = serde_json::to_string(&hint_bait).unwrap();

    let verdict = validate_envelope_verdict(&json);
    assert!(
        verdict.result.is_ok(),
        "hints must not reject: {:?}",
        verdict.result.err()
    );
    assert_eq!(verdict.hints.len(), 4, "{:?}", verdict.hints);
    // And the unchanged entry point agrees — its contract is the validity axis alone.
    assert!(validate_envelope(&json).is_ok());
}

/// Seals: hints survive an INVALID envelope, so a producer sees both axes in one round-trip instead of
/// one fix per call — except when the text never deserialized, where there is no envelope to inspect.
#[test]
fn hints_ride_alongside_issues_but_not_past_a_parse_failure() {
    let mut bad = envelope(vec![file("/abs/a.ts", IoFacts::default())]);
    bad.version = SUPPORTED_NORMALIZED_AST_VERSION + 1;
    let verdict = validate_envelope_verdict(&serde_json::to_string(&bad).unwrap());
    assert!(verdict.result.is_err());
    assert_eq!(verdict.hints.len(), 1, "{:?}", verdict.hints);

    let verdict = validate_envelope_verdict("not json");
    assert!(verdict.result.is_err());
    assert!(verdict.hints.is_empty());
}

/// Seals hint ORDER: deterministic and independent of hashing — declared file order, then provides
/// before consumes within a file. Repeated runs of the same input must be byte-identical.
#[test]
fn hint_order_is_deterministic() {
    let env = envelope(vec![
        file(
            "/abs/a.ts",
            io(
                vec![provide("http", "get /a", "a.ts", 1)],
                vec![consume("http", Some("get /b"), "a.ts", 2)],
            ),
        ),
        file(
            "/abs/b.ts",
            io(vec![provide("http", "get /c", "b.ts", 1)], vec![]),
        ),
    ]);
    let first = envelope_hints(&env);
    assert_eq!(first.len(), 5);
    assert!(first[0].contains("files[0].path"), "{first:?}");
    assert!(first[1].contains("provide at a.ts:1"), "{first:?}");
    assert!(first[2].contains("consume at a.ts:2"), "{first:?}");
    assert!(first[3].contains("files[1].path"), "{first:?}");
    for _ in 0..8 {
        assert_eq!(envelope_hints(&env), first);
    }
}

/// Seals the shipped fixtures against hint noise: the contract example `docs/NORMALIZED_AST.md` points
/// authors at must be hint-CLEAN, not merely valid — an example that trips the advisory pass teaches
/// the shape it warns about.
#[test]
fn the_jsp_contract_example_is_hint_clean() {
    let json = include_str!("../../../../docs/contracts/example-envelope.json");
    let verdict = validate_envelope_verdict(json);
    assert!(verdict.result.is_ok());
    assert!(verdict.hints.is_empty(), "{:?}", verdict.hints);
}
