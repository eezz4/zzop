//! The VOCABULARY `zzop graph` accepts: which picture (`GraphDomain`) and how it is serialized
//! (`GraphFormat`), plus the per-domain defaults and capability answers every surface derives from.
//!
//! # Why these are sealed enums and why they live together
//! Both are the same kind of thing — a closed set the CLI edge validates so an unknown value is a usage
//! error rather than a silently-empty diagram — and both are read by the SAME three places (the usage
//! line, the rejection message, and the dispatch). Keeping them in one file is what makes "who owns the
//! accepted set?" answerable by opening one file, which is the property that failed in v0.25.0 when the
//! domain list was spelled by hand in the usage line and derived in the rejection message: a caller was
//! told a domain does not exist and then told it does.

use super::{cochange, dep, posture, risk, DEFAULT_GRAPH_TOP};

/// How `zzop graph` serializes. Sealed for the same reason [`GraphDomain`] is: an unknown `--format` is a
/// usage error, never a silently-different output.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GraphFormat {
    /// The default and the only format the four domains all support.
    Mermaid,
    /// The dep domain's points table.
    CosmographNodes,
    /// The dep domain's links table — the one a viewer actually requires.
    CosmographLinks,
}

impl GraphFormat {
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "mermaid" => Some(GraphFormat::Mermaid),
            "cosmograph-nodes" => Some(GraphFormat::CosmographNodes),
            "cosmograph-links" => Some(GraphFormat::CosmographLinks),
            _ => None,
        }
    }

    pub const WIRE_NAMES: &[&str] = &["mermaid", "cosmograph-nodes", "cosmograph-links"];

    /// Is this a cosmograph table? One owner for the question, so the CLI's `--domain`/`--top`
    /// compatibility check cannot drift from the dispatch that follows it.
    pub fn is_cosmograph(self) -> bool {
        matches!(
            self,
            GraphFormat::CosmographNodes | GraphFormat::CosmographLinks
        )
    }
}

/// Which picture `zzop graph` draws. A sealed enum rather than a free string: an unknown `--domain`
/// must be a usage error at the CLI edge, never a silently-empty diagram.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GraphDomain {
    /// The cross-layer JOIN — nodes are io keys. The original domain; see this module's doc.
    Join,
    /// The FILE import graph — nodes are files, cycles drawn distinctly. See [`dep`].
    Dep,
    /// Blast-radius hubs + extraction seams. See [`risk`] for why the health SCORES are deliberately
    /// not drawn here.
    Risk,
    /// The mutating attack surface and its guard status. See [`posture`] for why a box means
    /// GUARDED-OR-EXEMPT rather than guarded.
    Posture,
    /// Git co-change — the second relation over the same nodes [`Dep`](GraphDomain::Dep) draws. Its OWN
    /// domain rather than an overlay, because an import edge is read from source while a co-change edge
    /// is a filtered sample of history; see [`cochange`] for why blending them would assert the two are
    /// the same kind of fact.
    CoChange,
}

impl GraphDomain {
    /// The wire spelling accepted by `--domain`. `None` for anything else, so the caller reports the
    /// full accepted set rather than guessing what was meant.
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "join" => Some(GraphDomain::Join),
            "dep" => Some(GraphDomain::Dep),
            "risk" => Some(GraphDomain::Risk),
            "posture" => Some(GraphDomain::Posture),
            "cochange" => Some(GraphDomain::CoChange),
            _ => None,
        }
    }

    /// Every accepted spelling, for the usage line — one owner, so a new domain cannot ship with a help
    /// text that does not mention it.
    pub const WIRE_NAMES: &[&str] = &["join", "dep", "risk", "posture", "cochange"];

    /// This domain's `--top` default. A join has tens of relations, an import graph has thousands, and
    /// one shared number would either black out the second or starve the first — so the caps genuinely
    /// differ, and the number a caller is told must be the number they get.
    ///
    /// The mapping lives HERE rather than in `graph_mermaid`'s match because a second reader exists:
    /// `zzop graph --help`. While the numbers sat in that match, the help text interpolated
    /// `DEFAULT_GRAPH_TOP` alone and told every caller the cap was 25 — true only for `join`, and
    /// silently wrong for the three domains D17 added (found by the v0.25.0 release audit). Both readers
    /// now derive from this one function, so a new domain cannot ship with a help text quoting some
    /// other domain's cap.
    pub fn default_top(self) -> usize {
        match self {
            GraphDomain::Join => DEFAULT_GRAPH_TOP,
            GraphDomain::Dep => dep::DEFAULT_DEP_TOP,
            GraphDomain::Risk => risk::DEFAULT_RISK_TOP,
            GraphDomain::Posture => posture::DEFAULT_POSTURE_TOP,
            GraphDomain::CoChange => cochange::DEFAULT_COCHANGE_TOP,
        }
    }

    /// Whether `--fold <n>` means anything for this domain — the ONE owner of that split, read by the
    /// CLI's refusal and by `graph_mermaid` alike.
    ///
    /// The line it draws is between a RELATION domain and a JUDGMENT domain. `dep`/`cochange` answer
    /// "what connects these two?" over nodes that ARE paths, so the granularity of a path is a free
    /// second axis. `risk`'s hubs and seams and `posture`'s routes are nodes the ENGINE picked by a
    /// judgment; folding them by path would either merge two separate verdicts into one box or leave the
    /// flag doing nothing. `join`'s nodes are io keys, which have no path granularity at all — a fold of
    /// the join is a real picture (fold the SITES on either side of each contract) but a different
    /// design than this one, and accepting the flag to do nothing would be the exact defect the message
    /// audit named.
    pub fn accepts_fold(self) -> bool {
        match self {
            GraphDomain::Dep | GraphDomain::CoChange => true,
            GraphDomain::Join | GraphDomain::Risk | GraphDomain::Posture => false,
        }
    }

    /// The domains that DO accept `--fold`, in `WIRE_NAMES` order — what a refusal message needs so it
    /// can name the alternatives instead of only saying no.
    pub fn fold_capable_names() -> Vec<&'static str> {
        Self::WIRE_NAMES
            .iter()
            .filter(|n| {
                Self::from_wire(n)
                    .expect("WIRE_NAMES and from_wire are the same vocabulary by construction")
                    .accepts_fold()
            })
            .copied()
            .collect()
    }

    /// `(wire name, default cap)` for every domain, in `WIRE_NAMES` order — what a help line needs to
    /// state the accepted set and each one's cap without copying either.
    pub fn wire_defaults() -> Vec<(&'static str, usize)> {
        Self::WIRE_NAMES
            .iter()
            .map(|n| {
                let d = Self::from_wire(n)
                    .expect("WIRE_NAMES and from_wire are the same vocabulary by construction");
                (*n, d.default_top())
            })
            .collect()
    }
}
