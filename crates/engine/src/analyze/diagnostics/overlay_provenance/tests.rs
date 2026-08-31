//! The axis: a reader must be able to tell extracted facts from declared ones, and a tripwire about
//! zzop's own sight must not be answered with a number that includes facts zzop was handed.

use super::*;

fn counts(http_provides: usize, http_consumes: usize) -> OverlayIoCounts {
    OverlayIoCounts {
        http_provides,
        http_consumes,
        other_provides: 0,
        other_consumes: 0,
    }
}

fn one(parser: &str, c: OverlayIoCounts) -> BTreeMap<String, OverlayIoCounts> {
    BTreeMap::from([(parser.to_string(), c)])
}

#[test]
fn a_run_with_no_overlays_says_nothing() {
    assert!(overlay_provenance_warning(&BTreeMap::new(), 12).is_none());
}

/// An overlay that carried only `attributes`/`is_entry` has no io provenance to disclose — its effect
/// shows up as the findings it clears. Emitting a provenance line naming zero facts would be noise on
/// exactly the adapters that are hardest to justify writing.
#[test]
fn an_overlay_that_contributed_no_io_says_nothing() {
    assert!(overlay_provenance_warning(&one("attrs-only/1", counts(0, 0)), 12).is_none());
}

#[test]
fn a_contributing_overlay_is_named_with_what_it_contributed() {
    let w = overlay_provenance_warning(&one("acme-adapter/1", counts(6, 2)), 20).unwrap();
    assert!(w.contains("`acme-adapter/1`"), "{w}");
    assert!(
        w.contains("6 http route(s)") && w.contains("2 http call(s)"),
        "{w}"
    );
    assert!(w.contains("6 came from an overlay"), "{w}");
    // The clause a reader needs most: these are the caller's facts, not zzop's.
    assert!(w.contains("CALLER declared"), "{w}");
}

/// The measured case, and the one where the share matters: every route in the tree came from the
/// adapter. Without saying so, a reader sees `provides: 6` and concludes zzop parses this framework.
#[test]
fn an_overlay_supplying_every_route_says_zzop_extracted_none() {
    let w = overlay_provenance_warning(&one("acme-adapter/1", counts(6, 0)), 6).unwrap();
    assert!(
        w.contains("every one of them") && w.contains("read no routes here at all"),
        "{w}"
    );
}

/// The invalidation: when the adapter is a minority contributor, the "zzop saw nothing" clause must
/// NOT appear. Otherwise every overlay run would read as total blindness.
#[test]
fn an_overlay_supplying_a_minority_of_routes_makes_no_blindness_claim() {
    let w = overlay_provenance_warning(&one("acme-adapter/1", counts(2, 0)), 40).unwrap();
    assert!(w.contains("2 came from an overlay"), "{w}");
    assert!(!w.contains("every one of them"), "{w}");
}

/// Multiple adapters are each named. A merged total would leave a reader unable to tell which adapter
/// to distrust when one of them turns out to be wrong.
#[test]
fn every_contributing_overlay_is_named_separately() {
    let overlay = BTreeMap::from([
        ("a-adapter/1".to_string(), counts(3, 0)),
        ("b-adapter/1".to_string(), counts(4, 0)),
    ]);
    let w = overlay_provenance_warning(&overlay, 7).unwrap();
    assert!(
        w.contains("`a-adapter/1`") && w.contains("`b-adapter/1`"),
        "{w}"
    );
    assert!(w.contains("7 came from an overlay"), "{w}");
}

#[test]
fn the_native_counts_subtract_what_the_overlays_supplied() {
    let overlay = one("acme-adapter/1", counts(6, 2));
    assert_eq!(native_http_provides(6, &overlay), 0);
    assert_eq!(native_http_provides(14, &overlay), 8);
    assert_eq!(native_http_consumes(5, &overlay), 3);
}

/// Dedup makes the overlay's own tally able to exceed what it actually ADDED (it re-emitted a fact the
/// native pass already had). Saturating is the safe direction: the worst case is a tripwire staying as
/// quiet as it was before, never one firing against a tree that really does have native routes.
#[test]
fn an_overlay_tally_larger_than_the_merged_total_floors_at_zero() {
    assert_eq!(native_http_provides(4, &one("dup/1", counts(6, 0))), 0);
}
