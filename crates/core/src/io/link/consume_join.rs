//! The consume side's STRUCTURAL join disposition — the one gate sequence every axis that asks
//! "can this consume be joined at all?" must run, in the one order that is a contract.
//!
//! ## Why this is a function and not a written-down convention
//! Four separate defects (absolute-URL egress, declared-host re-key, all-`{}` route identity, static
//! assets — the "A·N·O·P" run) had ONE shape: the multi-tree linker and the single-tree
//! `http/unprovided-consume` rule answered differently about the SAME fact, because the rule had
//! hand-copied a PREDICATE without the TRANSFORM that runs in front of it. Sharing
//! [`rekey_if_internal_host`] and [`crate::io::key_carries_route_identity`] closed two of them
//! predicate-by-predicate, but left the *sequencing* — "re-key BEFORE the `://` gate" — as prose that a
//! future caller can still get wrong. This function makes that impossible to express: the transform is
//! inside, so there is no way to ask for the gate without it.
//!
//! ## What is deliberately NOT here
//! **Vocabulary vetoes stay out** (`.svg` static assets, `/api`-ish segments, health paths). Per the
//! bucket-vs-rule layer split: the linker is the STRUCTURAL layer and is vocab-free — a fact readable
//! from the key or the file alone — while lexical judgment is the RULE's job and must not leak into
//! core (that is where the bucket's single definition comes from: it is join residue, not a filtered
//! opinion, which is why it ships with its own disclosure). This function is structural by construction:
//! it only ever
//! looks at "is there a key", "does the key carry a scheme/authority", and "is that authority one the
//! deployment declares as its own".
//!
//! **The provider lookup stays out too**, and with it the route-identity gate that depends on it. The
//! two axes look up providers in different spaces (every tree's provides vs one tree's own), and in the
//! linker the route-identity question is only asked of a MISS — a key with no route identity that
//! actually HITS a declared catch-all provide is a join, not a guess. Folding a lookup callback in here
//! to cover that would give both callers a shape neither wants; they call
//! [`crate::io::key_carries_route_identity`] directly on their own miss instead (already one shared
//! definition, and it is a pure predicate with no transform in front of it — the drift this module
//! exists to prevent cannot arise there).

use std::borrow::Cow;

use super::host_rekey::rekey_if_internal_host;

/// What the structural layer can say about one consume before any provider is looked up.
///
/// Returned by [`classify_consume_join`]; the ordering of the variants below is the gate order, and
/// that order is a contract (see each variant).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsumeJoin<'a> {
    /// The consume carries no resolved key at all (`IoConsume::key == None`) — a computed URL the
    /// extractor could not reduce to a literal. Nothing to join, and its absence is evidence of
    /// nothing: the linker buckets it as `unresolvedConsumes`, the single-tree rule skips it.
    Unresolved,
    /// The key still carries a scheme/authority (`"://"`) after the internal-host re-key had its
    /// chance — third-party egress, never joined against internal provides. Reachable ONLY when no
    /// re-key fired: a re-keyed key is a normalized path by construction (see
    /// [`classify_consume_join`]'s `debug_assert`), so this variant needs no re-key bookkeeping.
    External,
    /// The key survived every structural gate and is what a provider lookup must be keyed on.
    Joinable {
        /// The JOIN key. Borrowed from the input unless the internal-host re-key rewrote it, in which
        /// case it is the normalized internal path — never the absolute spelling. Downstream buckets
        /// and findings must report THIS key (keeping the original in `raw`), which is the linker's
        /// own bucket invariant.
        key: Cow<'a, str>,
        /// The literal [`LinkOptions::internal_hosts`] entry the re-key matched, when it fired —
        /// `None` on the ordinary (no declared hosts, or no match) path. Callers that disclose
        /// re-key effect count it; callers that don't, ignore it.
        ///
        /// [`LinkOptions::internal_hosts`]: super::LinkOptions::internal_hosts
        rekeyed_host: Option<String>,
    },
}

/// Runs the structural gate sequence over one consume's key: **no key → internal-host re-key →
/// `"://"` external egress**. Pure; `internal_hosts` is the [`LinkOptions::internal_hosts`] subset of
/// the link options (the only option any structural gate reads).
///
/// The re-key MUST run before the egress gate — an absolute-URL consume whose authority matches a host
/// the deployment declares as its own is a same-deployment call that merely spells its gateway host out
/// loud, not egress. Applying only the gate (the original `unprovided-consume` bug) vetoes exactly the
/// calls the multi-tree join happily reports. Declaring no hosts leaves every answer byte-identical to
/// the gate alone.
///
/// [`LinkOptions::internal_hosts`]: super::LinkOptions::internal_hosts
pub fn classify_consume_join<'a>(
    key: Option<&'a str>,
    internal_hosts: &[String],
) -> ConsumeJoin<'a> {
    let Some(key) = key else {
        return ConsumeJoin::Unresolved;
    };
    if let Some((rekeyed, host)) = rekey_if_internal_host(key, internal_hosts) {
        debug_assert!(
            !rekeyed.contains("://"),
            "re-key must yield a scheme-free path key (got {rekeyed:?}) — `External` below assumes a \
             re-keyed key can never need the egress gate, and the bucket invariant assumes it too"
        );
        return ConsumeJoin::Joinable {
            key: Cow::Owned(rekeyed),
            rekeyed_host: Some(host),
        };
    }
    if key.contains("://") {
        return ConsumeJoin::External;
    }
    ConsumeJoin::Joinable {
        key: Cow::Borrowed(key),
        rekeyed_host: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn joinable_key<'a>(j: &'a ConsumeJoin<'a>) -> &'a str {
        match j {
            ConsumeJoin::Joinable { key, .. } => key,
            other => panic!("expected Joinable, got {other:?}"),
        }
    }

    #[test]
    fn missing_key_is_unresolved_and_a_plain_path_is_joinable_verbatim() {
        assert_eq!(classify_consume_join(None, &[]), ConsumeJoin::Unresolved);
        let j = classify_consume_join(Some("GET /api/users"), &[]);
        assert_eq!(joinable_key(&j), "GET /api/users");
        assert!(matches!(
            j,
            ConsumeJoin::Joinable {
                rekeyed_host: None,
                ..
            }
        ));
    }

    #[test]
    fn absolute_url_is_external_unless_its_authority_is_a_declared_host() {
        assert_eq!(
            classify_consume_join(Some("GET https://vendor.com/api/users"), &[]),
            ConsumeJoin::External
        );
        // Same key, host declared: the transform runs FIRST, so the egress gate never sees a scheme.
        let hosts = vec!["vendor.com".to_string()];
        let j = classify_consume_join(Some("GET https://vendor.com/api/users"), &hosts);
        assert_eq!(joinable_key(&j), "GET /api/users");
        assert!(
            matches!(j, ConsumeJoin::Joinable { rekeyed_host: Some(ref h), .. } if h == "vendor.com")
        );
        // A non-matching declared host leaves the egress answer untouched.
        assert_eq!(
            classify_consume_join(
                Some("GET https://vendor.com/api/users"),
                &["other.com".into()]
            ),
            ConsumeJoin::External
        );
    }

    #[test]
    fn route_identity_is_not_this_gates_business() {
        // An all-`{}` key is `Joinable` here on purpose: whether its MISS means "drift" or "blind" is
        // decided AFTER a provider lookup, by `key_carries_route_identity` at each call site.
        let j = classify_consume_join(Some("GET /{}"), &[]);
        assert_eq!(joinable_key(&j), "GET /{}");
    }
}
