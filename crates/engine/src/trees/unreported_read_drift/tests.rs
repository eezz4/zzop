//! The axis: an empty findings list must not mean the same thing when nothing was FOUND and when
//! nothing was ELIGIBLE.

use super::*;

fn consume(key: &str) -> IoConsume {
    IoConsume {
        client: None,
        body: None,
        kind: "http".to_string(),
        key: Some(key.to_string()),
        file: "src/api.ts".to_string(),
        line: 1,
        raw: None,
        method: None,
        retry_configured: None,
    }
}

fn result(consumes: Vec<IoConsume>) -> CrossLayerResult {
    CrossLayerResult {
        unprovided_consumes: consumes
            .into_iter()
            .map(|consume| zzop_core::io::TaggedConsume {
                source: "fe".to_string(),
                consume,
            })
            .collect(),
        ..Default::default()
    }
}

#[test]
fn a_run_with_nothing_unprovided_says_nothing() {
    assert!(maybe_warn(&result(Vec::new())).is_none());
}

/// The two measured cases, both of which left `crossLayerFindings` empty.
#[test]
fn an_unprovided_get_is_named_along_with_the_gate_that_silenced_it() {
    let w = maybe_warn(&result(vec![
        consume("GET /api/auth/whoami"),
        consume("GET /api/ghost-endpoint"),
    ]))
    .expect("an unprovided read must be disclosed");
    assert!(w.contains("2 READ route(s)"), "{w}");
    assert!(w.contains("GET /api/auth/whoami"), "{w}");
    // Naming the gate is the point — otherwise this reads as a rule that failed rather than one that
    // declined, and a reader would go looking for a bug in the rule.
    assert!(w.contains("unprovided-mutation-call"), "{w}");
    assert!(w.contains("unprovided-consume"), "{w}");
    // And it must not read as a new judgment: the gates were deliberate and stay.
    assert!(
        w.contains("disclosure of a gate, not a new judgment"),
        "{w}"
    );
}

/// The invalidation: a WRITE verb already has a rule, so disclosing it here would double-report and
/// train readers to skim the channel.
#[test]
fn an_unprovided_write_is_left_to_the_rule_that_reports_it() {
    assert!(maybe_warn(&result(vec![
        consume("POST /api/orders"),
        consume("DELETE /api/orders/{}"),
    ]))
    .is_none());
}

/// A mixed run names only the reads. Without this, "disclose the ineligible ones" and "disclose every
/// unprovided consume" pass identically.
#[test]
fn a_mixed_run_names_only_the_reads() {
    let w = maybe_warn(&result(vec![
        consume("GET /api/ghost"),
        consume("POST /api/orders"),
        consume("HEAD /api/probe"),
    ]))
    .unwrap();
    assert!(w.contains("2 READ route(s)"), "{w}");
    assert!(!w.contains("POST /api/orders"), "{w}");
}

/// Deduped and capped, with the remainder counted — the same disclosure discipline every sampled list
/// in this crate follows.
#[test]
fn repeated_keys_count_once_and_a_long_list_discloses_its_own_truncation() {
    let w = maybe_warn(&result(vec![
        consume("GET /a"),
        consume("GET /a"),
        consume("GET /b"),
        consume("GET /c"),
        consume("GET /d"),
    ]))
    .unwrap();
    assert!(
        w.contains("4 READ route(s)"),
        "the duplicate counts once: {w}"
    );
    assert!(w.contains("and 1 more"), "{w}");
}

/// A verb this product does not recognize counts as a READ. That direction is deliberate: claiming the
/// write rule stayed silent about something it never saw would be a wrong statement about a rule,
/// where an over-broad disclosure is only a slightly wider list.
#[test]
fn an_unrecognized_verb_is_treated_as_a_read() {
    let w = maybe_warn(&result(vec![consume("OPTIONS /api/thing")])).unwrap();
    assert!(w.contains("1 READ route(s)"), "{w}");
}

/// Non-http consumes are another join entirely and have their own rules; pulling them in would make
/// this list mean something else.
#[test]
fn a_non_http_consume_is_not_in_this_census() {
    let mut c = consume("topic:orders");
    c.kind = "queue".to_string();
    assert!(maybe_warn(&result(vec![c])).is_none());
}
