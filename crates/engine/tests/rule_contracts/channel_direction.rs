//! The EXPERIMENT half of the rule→channel fact — what an empty io channel does to each rule that
//! reads it, derived by running the engine rather than by reading a declaration.
//!
//! # Why an experiment and not a field
//! `zzop_core::rule_channels` deliberately refuses to carry a `direction` field: a rule module's code
//! evidences which channels it reads and nothing at all about what their EMPTINESS does, so a
//! declared direction would be an unbindable field — hardcoding with a nicer address. The fact is
//! real all the same (`unconsumed-endpoint` goes quiet without http provides, `unprovided-consume`
//! floods), and a disclosure that names rules cannot be written without it. So it is measured.
//!
//! # The probe, and why its two arms differ by EXACTLY one channel
//! Each probe builds a run out of DONOR overlays: a real `cases/trees` fixture is analyzed on its own,
//! one io channel is lifted out of its extracted facts, and those facts are re-injected as a Mode B
//! adapter overlay (`EngineConfig::adapter_overlays`) onto a host directory that becomes its own join
//! source. The facts are therefore REAL extracted io that follows the fixture, not authored
//! constants. The probed channel's donors are attached in the SUPPLIED arm and withheld in the
//! WITHHELD arm; every other donor, the file set, the config and the rule gate are identical across
//! the two.
//!
//! Most hosts are EMPTY directories, so they contribute nothing of their own and withholding an
//! overlay withholds exactly one channel. The exception is [`SOURCE_BACKED_HOSTS`], and it exists
//! because a rule can read a channel through a second input an overlay cannot carry: the call-graph
//! family judges a route against the graph reachable from its handler, and that graph comes from a
//! re-parse of the tree's OWN sources. An empty host gives those rules an empty graph and therefore
//! equal, zero arms — ignorance, not an answer. A source-backed host still leaves the arms differing
//! by one channel, because what its files supply (symbols and call edges) is identical in both; the
//! floor leg below checks the one condition that makes that true — it must extract none of the
//! probed channel itself.
//!
//! That is the property "the same fixture, run twice" needs and that the obvious cheaper design —
//! dropping the provider tree — does not have: dropping a tree also drops its files, its symbols and
//! its own findings, so counts would move for reasons that are not the channel.
//! [`every_named_channel_has_a_probe_whose_arms_differ_by_exactly_that_channel`] PROVES the intended
//! difference rather than assuming it: zero facts of the probed channel run-wide when withheld, more
//! than zero when supplied.
//!
//! # How a verdict is decided, and why there is no threshold
//! Per rule, over the run's `cross_layer_findings` plus every tree's own `findings`: `Silences` =
//! withheld 0, supplied > 0. `Floods` = withheld > supplied — strictly MORE findings from LESS input,
//! a monotonicity violation that needs no magic number to recognize. `Reduces` = fewer but not none.
//! `Unobserved` = the probe moved the count not at all, which this fixture cannot turn into knowledge
//! and which is therefore published as ignorance rather than as an all-clear.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use zzop_core::recognizer::channel;
use zzop_core::rule_channels::reads;
use zzop_core::{
    FileProjection, Finding, IoConsume, IoFacts, IoProvide, NormalizedEnvelope, RuleIoChannel,
    NORMALIZED_AST_CONTRACT_VERSION, NORMALIZED_AST_FORMAT,
};
use zzop_engine::{analyze_trees, ChannelDirection, EngineConfig};

/// One donor: `(fixture tree, channel lifted out of its extracted io, channel it is re-injected as,
/// join source id)`.
///
/// The two channels are the same for every donor but one. `cases/trees` has no fixture that CONSUMES
/// a tRPC procedure — the `trpc` tree exists to leave one unconsumed, which is what
/// `cross-layer/unconsumed-procedure` is for, and giving it a caller would move that fixture's numbers
/// under every other test that reads it. So the trpc consume donor lifts that same tree's PROVIDE keys
/// and re-injects them as consumes: a call to a procedure some tree provides is exactly what the join
/// is for, and the keys stay real extracted output rather than string literals written here.
type Donor = (&'static str, RuleIoChannel, RuleIoChannel, &'static str);

/// One probe: the channel under test, and the donor set the run is built from.
struct Probe {
    channel: RuleIoChannel,
    donors: &'static [Donor],
}

/// `xlayer-be`/`xlayer-be2`/`xlayer-fe` are the repo's purpose-built cross-layer set — three sources,
/// so ambiguity, shadowing and duplicate-route have something to collide over.
const HTTP_DONORS: &[Donor] = &[
    (
        "xlayer-be",
        reads::HTTP_PROVIDES,
        reads::HTTP_PROVIDES,
        "probe-be",
    ),
    (
        "xlayer-be2",
        reads::HTTP_PROVIDES,
        reads::HTTP_PROVIDES,
        "probe-be2",
    ),
    (
        "xlayer-fe",
        reads::HTTP_CONSUMES,
        reads::HTTP_CONSUMES,
        "probe-fe",
    ),
    // ...and one host carrying BOTH sides at once, because six of the io-reading rules are per-TREE
    // (`unprovided-consume`, `duplicate-route`, `route-shadowing`, ...): they judge one tree's own
    // provides against its own consumes, so a run that keeps the two sides in separate sources moves
    // them not at all and would measure every one of them `Unobserved`. Same donors, same lift — only
    // the host they land on differs.
    (
        "xlayer-be",
        reads::HTTP_PROVIDES,
        reads::HTTP_PROVIDES,
        "probe-solo",
    ),
    (
        "xlayer-be2",
        reads::HTTP_PROVIDES,
        reads::HTTP_PROVIDES,
        "probe-solo",
    ),
    (
        "xlayer-fe",
        reads::HTTP_CONSUMES,
        reads::HTTP_CONSUMES,
        "probe-solo",
    ),
    // ...and one host that is NOT an empty directory (see [`SOURCE_BACKED_HOSTS`]), because the
    // call-graph-BFS family reads a SECOND input no overlay can carry. `mutating-route-no-auth`,
    // `unsafe-read-endpoint` and `non-idempotent-write` judge a route against the call graph reachable
    // from its handler, and that graph comes from a re-parse of the tree's own sources
    // (`analyze::native_rules::callgraph` — `FileArtifact` carries no `RawCall`s), while Mode B
    // explicitly drops the envelope `calls` channel with a warning. On an empty host all three
    // therefore ran over an empty graph and reported nothing in BOTH arms, which is only publishable
    // as `Unobserved`. Lifting the ROUTE out of `callgraph` and leaving its HANDLERS on disk as
    // `callgraph-handlers` makes the route the one thing that differs between the arms, with the code
    // real and identical in both.
    (
        "callgraph",
        reads::HTTP_PROVIDES,
        reads::HTTP_PROVIDES,
        "probe-cg",
    ),
];

/// Hosts whose root is a real `cases/trees` fixture rather than an empty scratch directory, and the
/// fixture each one uses.
///
/// Every other host contributes nothing of its own on purpose — that is what makes withholding an
/// overlay withhold exactly one channel. A host in this table breaks that default deliberately and
/// under one condition, checked by the floor test rather than trusted: **it must extract none of the
/// probed channel itself**. `callgraph-handlers` qualifies because the route registration is precisely
/// what was lifted out of it; its files supply only symbols and call edges, which both arms get
/// identically, so the arms still differ by the injected channel alone.
const SOURCE_BACKED_HOSTS: &[(&str, &str)] = &[("probe-cg", "callgraph-handlers")];

/// `api-be` is the only `cases/trees` fixture carrying both db-table sides (a Prisma schema plus query
/// call sites), so both donors are lifted from it into two distinct join sources.
const DB_DONORS: &[Donor] = &[
    (
        "api-be",
        reads::DB_TABLE_PROVIDES,
        reads::DB_TABLE_PROVIDES,
        "probe-db-provider",
    ),
    (
        "api-be",
        reads::DB_TABLE_CONSUMES,
        reads::DB_TABLE_CONSUMES,
        "probe-db-consumer",
    ),
    // The tables `api-be` WRITES and the tables it READS are disjoint sets in that fixture, so the two
    // donors above never meet on a key. This third one re-injects the provide keys as consumes from a
    // separate source, which is the only shape `db-table-name-in-multiple-sources` judges.
    (
        "api-be",
        reads::DB_TABLE_PROVIDES,
        reads::DB_TABLE_CONSUMES,
        "probe-db-caller",
    ),
];

/// The provide-side trpc probe deliberately runs with NO consumer: `unconsumed-procedure` is the only
/// rule reading that channel, and an unconsumed procedure is the thing it reports — a run that supplies
/// a caller makes it silent in BOTH arms and measures nothing. The consume-side probe below needs both.
const TRPC_PROVIDE_DONORS: &[Donor] = &[(
    "trpc",
    reads::TRPC_PROVIDES,
    reads::TRPC_PROVIDES,
    "probe-trpc-provider",
)];

const TRPC_CONSUME_DONORS: &[Donor] = &[
    (
        "trpc",
        reads::TRPC_PROVIDES,
        reads::TRPC_PROVIDES,
        "probe-trpc-provider",
    ),
    (
        "trpc",
        reads::TRPC_PROVIDES,
        reads::TRPC_CONSUMES,
        "probe-trpc-consumer",
    ),
];

/// One probe per named channel — held to that by the floor test, so a channel cannot be added and
/// left unmeasured.
const PROBES: &[Probe] = &[
    Probe {
        channel: reads::HTTP_PROVIDES,
        donors: HTTP_DONORS,
    },
    Probe {
        channel: reads::HTTP_CONSUMES,
        donors: HTTP_DONORS,
    },
    Probe {
        channel: reads::DB_TABLE_PROVIDES,
        donors: DB_DONORS,
    },
    Probe {
        channel: reads::DB_TABLE_CONSUMES,
        donors: DB_DONORS,
    },
    Probe {
        channel: reads::TRPC_PROVIDES,
        donors: TRPC_PROVIDE_DONORS,
    },
    Probe {
        channel: reads::TRPC_CONSUMES,
        donors: TRPC_CONSUME_DONORS,
    },
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// ---------------------------------------------------------------------------------------------
// Donor extraction — real facts, lifted from a real run
// ---------------------------------------------------------------------------------------------

/// Every donor overlay, built once. Each fixture is analyzed exactly once per lifted channel; the
/// probes below then reuse the envelopes, so the twelve arms cost twelve joins, not twelve re-parses.
fn overlays() -> &'static BTreeMap<Donor, NormalizedEnvelope> {
    static CACHE: OnceLock<BTreeMap<Donor, NormalizedEnvelope>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut out = BTreeMap::new();
        for probe in PROBES {
            for donor in probe.donors {
                let (fixture, lift, inject, host) = *donor;
                out.entry(*donor)
                    .or_insert_with(|| donor_overlay(fixture, lift, inject, host));
            }
        }
        out
    })
}

/// The `lift` slice of one fixture tree's extracted io, re-injected as `inject` in an overlay envelope
/// attached to `host` — one `FileProjection` per file the facts came from, carrying those facts and
/// nothing else.
///
/// A same-side donor clones the extracted fact WHOLE, so the optional channels several rules judge on
/// (`body`, `response`, `retryConfigured`) survive the lift; a cross-side donor can only carry the join
/// key, method and anchor, which is all a side change has any meaning for.
fn donor_overlay(
    fixture: &str,
    lift: RuleIoChannel,
    inject: RuleIoChannel,
    host: &str,
) -> NormalizedEnvelope {
    let root = repo_root().join("cases/trees").join(fixture);
    let out = analyze_trees(&[(
        root,
        EngineConfig {
            source_id: format!("donor-{fixture}"),
            ..EngineConfig::default()
        },
    )]);
    let io = out.trees[0].2.ir.ir.io.clone().unwrap_or_default();
    let mut by_file: BTreeMap<String, IoFacts> = BTreeMap::new();
    let same_side = lift.side == inject.side;
    for p in io.provides.iter().filter(|p| p.kind == lift.kind) {
        if lift.side != channel::PROVIDES {
            continue;
        }
        let slot = by_file.entry(p.file.clone()).or_default();
        if same_side {
            slot.provides.push(p.clone());
        } else {
            slot.consumes.push(IoConsume {
                kind: inject.kind.to_string(),
                key: Some(p.key.clone()),
                file: p.file.clone(),
                line: p.line,
                raw: None,
                method: None,
                body: None,
                client: None,
                retry_configured: None,
            });
        }
    }
    for c in io.consumes.iter().filter(|c| c.kind == lift.kind) {
        if lift.side != channel::CONSUMES {
            continue;
        }
        let slot = by_file.entry(c.file.clone()).or_default();
        if same_side {
            slot.consumes.push(c.clone());
        } else {
            let Some(key) = c.key.clone() else { continue };
            slot.provides.push(IoProvide {
                kind: inject.kind.to_string(),
                key,
                file: c.file.clone(),
                line: c.line,
                symbol: None,
                body: None,
                response: None,
            });
        }
    }
    NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: NORMALIZED_AST_CONTRACT_VERSION.to_string(),
        parser: format!("channel-direction-probe-{fixture}/1"),
        source: host.to_string(),
        files: by_file
            .into_iter()
            .map(|(path, io)| FileProjection {
                path,
                loc: 1,
                io,
                ..FileProjection::default()
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------------------------
// The two arms
// ---------------------------------------------------------------------------------------------

/// An empty directory per host source id — the hosts carry no source of their own on purpose, so
/// withholding an overlay withholds exactly that channel and nothing else. Under `target/` because it
/// is build output, not a fixture anyone reads.
///
/// The exception is [`SOURCE_BACKED_HOSTS`], whose roots are real fixtures: a rule that reads the call
/// graph has no answer at all on a host with no code to parse.
fn host_root(host: &str) -> PathBuf {
    if let Some((_, fixture)) = SOURCE_BACKED_HOSTS.iter().find(|(h, _)| *h == host) {
        return repo_root().join("cases/trees").join(fixture);
    }
    let dir = repo_root()
        .join("target/channel-direction-hosts")
        .join(host);
    std::fs::create_dir_all(&dir).expect("probe host dir");
    dir
}

/// One arm's result: findings by rule, and how many facts of each named channel the run really had.
struct Arm {
    counts: BTreeMap<String, usize>,
    channel_facts: BTreeMap<String, usize>,
}

/// Runs one arm — every donor attached except those whose channel is `withheld`.
fn run_arm(donors: &[Donor], withheld: Option<RuleIoChannel>) -> Arm {
    // Grouped by HOST, because several donors may land on one tree (see `probe-solo`): one tree per
    // host, carrying every donor of that host whose injected channel is not the withheld one.
    let mut by_host: BTreeMap<&'static str, Vec<NormalizedEnvelope>> = BTreeMap::new();
    for donor in donors {
        let (_, _, inject, host) = *donor;
        let slot = by_host.entry(host).or_default();
        if Some(inject) != withheld {
            slot.push(overlays()[donor].clone());
        }
    }
    let trees: Vec<(PathBuf, EngineConfig)> = by_host
        .into_iter()
        .map(|(host, adapter_overlays)| {
            (
                host_root(host),
                EngineConfig {
                    source_id: host.to_string(),
                    adapter_overlays,
                    ..EngineConfig::default()
                },
            )
        })
        .collect();
    let out = analyze_trees(&trees);

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let tally = |counts: &mut BTreeMap<String, usize>, findings: &[Finding]| {
        for f in findings {
            *counts.entry(f.rule_id.clone()).or_insert(0) += 1;
        }
    };
    tally(&mut counts, &out.cross_layer_findings);
    let mut channel_facts: BTreeMap<String, usize> =
        reads::ALL.iter().map(|c| (c.label(), 0)).collect();
    for (_, _, tree) in &out.trees {
        tally(&mut counts, &tree.findings);
        let io = tree.ir.ir.io.clone().unwrap_or_default();
        for c in reads::ALL {
            let n = if c.side == channel::PROVIDES {
                io.provides.iter().filter(|p| p.kind == c.kind).count()
            } else {
                io.consumes.iter().filter(|x| x.kind == c.kind).count()
            };
            *channel_facts.get_mut(&c.label()).expect("named channel") += n;
        }
    }
    Arm {
        counts,
        channel_facts,
    }
}

fn verdict(withheld: usize, supplied: usize) -> ChannelDirection {
    match (withheld, supplied) {
        (w, s) if w == s => ChannelDirection::Unobserved,
        (w, s) if w > s => ChannelDirection::Floods,
        (0, _) => ChannelDirection::Silences,
        _ => ChannelDirection::Reduces,
    }
}

/// `(rule_id, channel label) -> measured direction`, for every declared pair.
type Derivation = BTreeMap<(String, String), ChannelDirection>;

/// One probe's own validity evidence: `(channel label, facts in the withheld arm, facts in the
/// supplied arm)`.
type ProbeTotals = (String, usize, usize);

/// The whole derivation, plus the per-probe arm fact totals the floor leg judges.
fn derive() -> (Derivation, Vec<ProbeTotals>) {
    let mut declared: BTreeMap<String, BTreeSet<RuleIoChannel>> = BTreeMap::new();
    for row in zzop_engine::native_rule_channels() {
        declared
            .entry(row.rule_id)
            .or_default()
            .extend(row.reads.iter().copied());
    }

    let mut out = BTreeMap::new();
    let mut totals = Vec::new();
    for probe in PROBES {
        let label = probe.channel.label();
        let supplied = run_arm(probe.donors, None);
        let withheld = run_arm(probe.donors, Some(probe.channel));
        totals.push((
            label.clone(),
            withheld.channel_facts[&label],
            supplied.channel_facts[&label],
        ));
        for (rule_id, channels) in &declared {
            if !channels.contains(&probe.channel) {
                continue;
            }
            let s = supplied.counts.get(rule_id).copied().unwrap_or(0);
            let w = withheld.counts.get(rule_id).copied().unwrap_or(0);
            out.insert((rule_id.clone(), label.clone()), verdict(w, s));
        }
    }
    (out, totals)
}

fn spelling(direction: ChannelDirection) -> &'static str {
    match direction {
        ChannelDirection::Silences => "Silences",
        ChannelDirection::Floods => "Floods",
        ChannelDirection::Reduces => "Reduces",
        ChannelDirection::Unobserved => "Unobserved",
    }
}

/// The `reads::*` constant whose label is `label` — the spelling the pasteable block must use.
fn channel_const(label: &str) -> &'static str {
    const NAMES: &[(&str, RuleIoChannel)] = &[
        ("HTTP_PROVIDES", reads::HTTP_PROVIDES),
        ("HTTP_CONSUMES", reads::HTTP_CONSUMES),
        ("DB_TABLE_PROVIDES", reads::DB_TABLE_PROVIDES),
        ("DB_TABLE_CONSUMES", reads::DB_TABLE_CONSUMES),
        ("TRPC_PROVIDES", reads::TRPC_PROVIDES),
        ("TRPC_CONSUMES", reads::TRPC_CONSUMES),
    ];
    NAMES
        .iter()
        .find(|(_, c)| c.label() == label)
        .map(|(n, _)| *n)
        .expect("every derived label is a named channel")
}

// ---------------------------------------------------------------------------------------------
// The contract, and the floor beneath it
// ---------------------------------------------------------------------------------------------

/// THE BINDING, and the floor that keeps it from being vacuous, in ONE test because they share the
/// twelve engine runs the derivation costs.
///
/// Floor: every named channel has a probe, and each probe's arms really do differ by that channel and
/// only through it — zero facts of it when withheld, more than zero when supplied. Without that leg a
/// donor that stopped extracting anything would derive `Unobserved` for every rule and the table would
/// agree with it.
///
/// Binding: the shipped table equals the derivation row for row. This is what makes
/// `zzop_engine::channel_direction`'s table a cache of a measurement rather than a second hand-written
/// declaration — a rule whose response to an empty channel changes goes red here, and the failure
/// message carries the corrected block ready to paste.
#[test]
fn the_shipped_direction_table_is_exactly_what_the_probe_measures() {
    let probed: BTreeSet<String> = PROBES.iter().map(|p| p.channel.label()).collect();
    let named: BTreeSet<String> = reads::ALL.iter().map(|c| c.label()).collect();
    assert_eq!(
        probed, named,
        "a named io channel has no direction probe — its rules' directions would be derived from \
         nothing and published as `Unobserved`, which reads as knowledge"
    );

    let (derived, totals) = derive();
    for (label, withheld, supplied) in &totals {
        assert_eq!(
            *withheld, 0,
            "{label}: the withheld arm still carries {withheld} fact(s) of the probed channel — the \
             two arms do not differ by that channel, so every verdict derived from them is unfounded"
        );
        assert!(
            *supplied > 0,
            "{label}: the supplied arm carries no fact of the probed channel either — the donor \
             fixture stopped extracting it, so this probe measures nothing at all"
        );
    }

    let shipped: BTreeMap<(String, String), ChannelDirection> = zzop_engine::channel_directions()
        .iter()
        .map(|r| ((r.rule_id.to_string(), r.channel.label()), r.direction))
        .collect();
    let mut wrong = Vec::new();
    for (key, direction) in &derived {
        match shipped.get(key) {
            Some(s) if s == direction => {}
            Some(s) => wrong.push(format!(
                "{} / {}: table says {}, probe measured {}",
                key.0,
                key.1,
                spelling(*s),
                spelling(*direction)
            )),
            None => wrong.push(format!("{} / {}: no row in the table", key.0, key.1)),
        }
    }
    for key in shipped.keys() {
        if !derived.contains_key(key) {
            wrong.push(format!(
                "{} / {}: a row nothing declares or nothing probes",
                key.0, key.1
            ));
        }
    }

    let block: Vec<String> = derived
        .iter()
        .map(|((rule, label), d)| {
            format!(
                "    row(\"{rule}\", {}, {}),",
                channel_const(label),
                spelling(*d)
            )
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "the shipped direction table disagrees with the measurement:\n{}\n\n\
         Replace `OBSERVED` in crates/engine/src/channel_direction/table.rs with:\n{}",
        wrong.join("\n"),
        block.join("\n")
    );
}
