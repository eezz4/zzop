use std::collections::{BTreeSet, HashMap};

use zzop_core::callgraph::SymbolEdge;
use zzop_core::{SourceSymbol, SourceSymbolKind};

use super::*;

fn sym(id: &str, name: &str, file: &str, line: u32) -> SourceSymbol {
    SourceSymbol {
        id: id.to_string(),
        file: file.to_string(),
        name: name.to_string(),
        kind: SourceSymbolKind::Function,
        line,
        exported: false,
        is_default: false,
        body_start: None,
        body_end: None,
        write_sites: Vec::new(),
    }
}

fn edge(from: &str, to: &str) -> SymbolEdge {
    SymbolEdge {
        from: from.to_string(),
        to: to.to_string(),
    }
}

const READS: &[&str] = &["read_to_string", "read_dir"];
const ANCHOR: &str = "^compute_fresh_artifact$";

struct Fixture {
    symbols: Vec<SourceSymbol>,
    graph: Vec<SymbolEdge>,
    calls: Vec<(String, Vec<String>)>,
}

impl Fixture {
    fn run(&self) -> Vec<Finding> {
        let call_sites: CacheLaneCallSites = self
            .calls
            .iter()
            .map(|(from, names)| {
                (
                    from.as_str(),
                    names.iter().map(String::as_str).collect::<BTreeSet<&str>>(),
                )
            })
            .collect::<HashMap<_, _>>();
        scan_cache_lane_file_read(&ScanCacheLaneFileReadInput {
            symbols: &self.symbols,
            symbol_graph: &self.graph,
            call_sites: &call_sites,
            cache_lane_anchor_pattern: Some(ANCHOR),
            file_read_callees: READS,
        })
    }
}

/// The lane calls a helper in ANOTHER file that reads. This is the shape a textual guard on the lane's
/// own file cannot see, and the reason this is a rule rather than a script.
fn cross_file_read() -> Fixture {
    Fixture {
        symbols: vec![
            sym(
                "a.rs#compute_fresh_artifact",
                "compute_fresh_artifact",
                "a.rs",
                10,
            ),
            sym("b.rs#load_manifest", "load_manifest", "b.rs", 3),
        ],
        graph: vec![edge("a.rs#compute_fresh_artifact", "b.rs#load_manifest")],
        calls: vec![(
            "b.rs#load_manifest".to_string(),
            vec!["read_to_string".to_string()],
        )],
    }
}

#[test]
fn a_read_one_hop_outside_the_lanes_own_file_is_found() {
    let found = cache_lane_findings(&cross_file_read());
    assert_eq!(found.len(), 1, "{found:?}");
    let d = found[0].data.as_ref().unwrap();
    assert_eq!(d["reachedSymbol"], "b.rs#load_manifest");
    assert_eq!(d["callee"], "read_to_string");
    assert_eq!(d["depth"], 1);
    // The finding anchors on the LANE, not the helper — the lane is what carries the closure promise.
    assert_eq!(found[0].file, "a.rs");
    assert_eq!(found[0].line, 10);
}

fn cache_lane_findings(f: &Fixture) -> Vec<Finding> {
    f.run()
}

#[test]
fn a_direct_read_in_the_lane_itself_reports_depth_zero() {
    let f = Fixture {
        symbols: vec![sym(
            "a.rs#compute_fresh_artifact",
            "compute_fresh_artifact",
            "a.rs",
            1,
        )],
        graph: Vec::new(),
        calls: vec![(
            "a.rs#compute_fresh_artifact".to_string(),
            vec!["read_dir".to_string()],
        )],
    };
    let found = f.run();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].data.as_ref().unwrap()["depth"], 0);
    assert!(
        found[0].message.contains("itself calls"),
        "{}",
        found[0].message
    );
}

#[test]
fn a_lane_that_reaches_no_read_is_silent() {
    let f = Fixture {
        symbols: vec![
            sym(
                "a.rs#compute_fresh_artifact",
                "compute_fresh_artifact",
                "a.rs",
                1,
            ),
            sym("b.rs#parse_ts", "parse_ts", "b.rs", 1),
        ],
        graph: vec![edge("a.rs#compute_fresh_artifact", "b.rs#parse_ts")],
        calls: vec![("b.rs#parse_ts".to_string(), vec!["push".to_string()])],
    };
    assert!(f.run().is_empty());
}

/// A read that exists in the tree but is NOT reachable from the lane is somebody else's business — the
/// rule claims reachability, so an unreachable read must not be reported as one.
#[test]
fn a_read_that_the_lane_cannot_reach_is_not_the_lanes_problem() {
    let f = Fixture {
        symbols: vec![
            sym(
                "a.rs#compute_fresh_artifact",
                "compute_fresh_artifact",
                "a.rs",
                1,
            ),
            sym("c.rs#unrelated", "unrelated", "c.rs", 1),
        ],
        graph: Vec::new(),
        calls: vec![(
            "c.rs#unrelated".to_string(),
            vec!["read_to_string".to_string()],
        )],
    };
    assert!(f.run().is_empty());
}

/// D14, literally: an undeclared vocabulary means the judgment is NOT MADE. Both halves, because either
/// one silently defaulting would report a function nobody promised was pure.
#[test]
fn an_undeclared_anchor_or_sink_vocabulary_makes_no_judgment() {
    let f = cross_file_read();
    let call_sites: CacheLaneCallSites = f
        .calls
        .iter()
        .map(|(from, names)| {
            (
                from.as_str(),
                names.iter().map(String::as_str).collect::<BTreeSet<&str>>(),
            )
        })
        .collect::<HashMap<_, _>>();
    let base = ScanCacheLaneFileReadInput {
        symbols: &f.symbols,
        symbol_graph: &f.graph,
        call_sites: &call_sites,
        cache_lane_anchor_pattern: Some(ANCHOR),
        file_read_callees: READS,
    };
    // The control: with both declared it DOES fire, so the two assertions below prove the vocabulary is
    // what silenced it and not the fixture.
    assert_eq!(scan_cache_lane_file_read(&base).len(), 1);

    assert!(scan_cache_lane_file_read(&ScanCacheLaneFileReadInput {
        cache_lane_anchor_pattern: None,
        ..base
    })
    .is_empty());
    assert!(scan_cache_lane_file_read(&ScanCacheLaneFileReadInput {
        cache_lane_anchor_pattern: Some(""),
        ..base
    })
    .is_empty());
    assert!(scan_cache_lane_file_read(&ScanCacheLaneFileReadInput {
        file_read_callees: &[],
        ..base
    })
    .is_empty());
}

/// A pattern that does not compile is a declaration this run cannot honor — it must not silently fall
/// back to some built-in anchor set, which would judge symbols the author never nominated.
#[test]
fn an_uncompilable_anchor_pattern_judges_nothing_rather_than_falling_back() {
    let f = cross_file_read();
    let call_sites: CacheLaneCallSites = f
        .calls
        .iter()
        .map(|(from, names)| {
            (
                from.as_str(),
                names.iter().map(String::as_str).collect::<BTreeSet<&str>>(),
            )
        })
        .collect::<HashMap<_, _>>();
    assert!(scan_cache_lane_file_read(&ScanCacheLaneFileReadInput {
        symbols: &f.symbols,
        symbol_graph: &f.graph,
        call_sites: &call_sites,
        cache_lane_anchor_pattern: Some("("),
        file_read_callees: READS,
    })
    .is_empty());
}

/// The message must name the escape hatch that is usually CORRECT — "put what you read into the key" —
/// not only "delete the read". A message that offered one way out would push authors toward the wrong
/// one half the time.
#[test]
fn the_message_offers_keying_the_read_and_not_only_removing_it() {
    let found = cross_file_read().run();
    let m = &found[0].message;
    assert!(
        m.contains("part of \nthe key") || m.contains("part of the key"),
        "{m}"
    );
    assert!(m.contains("vocabulary.fileReadCallees"), "{m}");
    assert!(
        m.contains("load_manifest") && m.contains("read_to_string"),
        "{m}"
    );
}
