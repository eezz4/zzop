//! The envelope CONTRACT-VERSION vocabulary — the format string, the shape version a conforming
//! producer declares, the per-feature minimum floors, this build's own ceiling, and the ordering
//! comparison all four are read through. Split out of `normalized.rs` so that file holds the WIRE TYPES
//! and this one holds the question "which contract is this, and is it one we accept" — the two change
//! for unrelated reasons (a new FIELD vs. a new RELEASE), and the version prose here is long enough that
//! keeping both in one file put the wire types past this repo's 300-line limit.
//!
//! `docs/NORMALIZED_AST.md` is the public owner of everything below; these docs say what the CODE does
//! with each value, never a second copy of the contract itself.

/// The exact `format` string every conforming envelope must carry (`docs/NORMALIZED_AST.md`'s Envelope
/// section).
pub const NORMALIZED_AST_FORMAT: &str = "zzop-normalized-ast";

/// The RELEASE in which the envelope shape last changed — the value a conforming producer declares as
/// `NormalizedEnvelope::version`, and the only contract-version constant this crate has.
///
/// ## Why this is a release number and not a counter (2026-07-31, user ruling)
/// It used to be an independent `u32` (v1, v2), so a reader had to hold two unrelated version systems
/// at once and could not tell from an envelope which zzop it belonged with. It now uses the workspace's
/// own semver, so "which zzop is this" and "which shape is this" are answered in the same units.
///
/// It is NOT simply the current release. It moves only when the SHAPE moves, which is what keeps a
/// producer's output working across releases: an adapter that emitted `"0.27.0"` keeps emitting it, and
/// keeps being accepted, through every later release that did not change the shape. Bumping it every
/// release would be the defect this replaces — a number that appears to describe the shape while
/// actually describing the calendar, leaving a reader unable to tell which bump mattered.
///
/// ⚠ That is exactly what happened during the v0.28.0 bump and it was caught in the pre-tag audit: this
/// constant AND `docs/contracts/example-envelope.json` were moved to `0.28.0` for a release whose only
/// envelope diff was a prose `description` rewrite, while the schema and `NORMALIZED_AST.md` still (and
/// correctly) said `0.27.0`. An adapter author copying the shipped example would have emitted an
/// envelope every 0.27.x engine rejects, for a shape identical to the one they already had. The example
/// file is now excluded from `scripts/check-release-version-propagation.sh` for this reason — a
/// release-propagation guard must not reach a constant whose contract is "do not track the release".
///
/// It stands at `0.29.0` because the SHAPE moved, the only reason it ever moves:
/// [`crate::RouterMountEntry`] gained a `MountRef` variant. A new variant on a wire enum is a new shape
/// in the strongest sense available here — the enum is externally tagged with no catch-all arm, so an
/// engine predating the variant fails the whole envelope at deserialization. A producer emitting one is
/// not describing `0.27.0`'s contract. This move is what the ⚠ above is not: that one was the calendar.
/// The same (unreleased) `0.29.0` also carries `FileProjection::calls` — a droppable field rather than
/// a variant, so it additionally gets a floor ([`MIN_VERSION_FOR_CALLS`]), per the policy below.
///
/// ## What acceptance means
/// A consumer accepts an envelope whose declared version is `<=` its own package version, and rejects
/// anything newer ("reject newer, never guess" — the same policy `pack_loader`'s DSL schema version
/// keeps). An engine built before a shape existed refuses the whole envelope rather than silently
/// ignoring the field it does not know, which is the silent-loss shape the displacement disclosure
/// exists to abolish, reappearing one level up in the contract. `FileProjection` has no
/// `deny_unknown_fields`, so without this comparison an unknown field would deserialize, be dropped,
/// and leave the producer believing it applied.
///
/// That comparison protects an HONEST producer, and only an honest one — it cannot see a mislabelled
/// envelope, which declares an old version while carrying a new field. Nothing about switching to a
/// release number changed that, so the per-feature floors stay ([`MIN_VERSION_FOR_OVERRIDES`] is the
/// enforced one; [`MIN_VERSION_FOR_ROUTER_MOUNT_REF`] records a floor serde already enforces): for a
/// silently-droppable FIELD the floor is the only thing that makes the mislabel fail loudly on the
/// engine that DOES understand it, which is the one run where the producer can still be told.
///
/// PRE-1.0 CONSEQUENCE, accepted deliberately: an envelope declaring this version does not run on an
/// engine older than it, including engines that would have understood every field in it. `VERSIONING.md`
/// already states that `0.x` makes no backward-compatibility promise; the only known producers are this
/// repo's own `examples/adapters/`, which are migrated in the same commit.
pub const NORMALIZED_AST_CONTRACT_VERSION: &str = "0.29.0";

/// The release that introduced `overrides` — the floor an envelope must DECLARE to use it.
///
/// A per-feature floor is not made redundant by the `<=` acceptance comparison above, because the two
/// catch opposite mistakes. Acceptance catches an envelope that is NEWER than the engine. This catches
/// one that claims to be OLDER than the field it carries: declared `"0.20.0"` plus a populated
/// `overrides` deserializes cleanly on an engine that predates the field, drops it, and produces a run
/// where the adapter believes it displaced a native binding and the engine quietly did not. The engine
/// that understands the field is the only one positioned to notice, so it rejects — the producer learns
/// at authoring time instead of shipping bytes that mean different things to different engines.
///
/// A new gated field adds a constant here and moves [`NORMALIZED_AST_CONTRACT_VERSION`] to the same
/// release. Fields that are safe to silently ignore need no floor and get none.
pub const MIN_VERSION_FOR_OVERRIDES: &str = "0.27.0";

/// The release that introduced `FileProjection::calls` — the floor an envelope must DECLARE to use it.
///
/// Same mechanism, same rationale as [`MIN_VERSION_FOR_OVERRIDES`] directly above: `calls` is a
/// silently-droppable struct FIELD, so an envelope declaring an older version while carrying it would
/// deserialize cleanly on an engine predating the field, drop it, and leave the producer believing its
/// call graph applied while every call-graph rule stayed silent — a RECALL loss with no trace, the
/// exact silent-failure class the channel's own absence disclosure exists to abolish. The engine that
/// understands the field rejects the mislabel so the producer learns at authoring time.
pub const MIN_VERSION_FOR_CALLS: &str = "0.29.0";

/// The release that introduced [`crate::RouterMountEntry::MountRef`] — the floor from which a consumer
/// may expect that variant. It is pinned to the release that introduced it and a later contract bump
/// must NOT carry it along: a floor that tracks [`NORMALIZED_AST_CONTRACT_VERSION`] answers "what is
/// current" to a reader who asked "since when", which is not a floor.
///
/// Unlike [`MIN_VERSION_FOR_OVERRIDES`] it is not checked in `validate_envelope`, for a reason in the
/// wire shape and not in how much it matters: `overrides` is a struct FIELD, dropped in silence by an
/// engine predating it, so only a hand-written floor makes a mislabelled envelope (old declared
/// version, new payload) fail. A new enum VARIANT cannot be dropped — an engine predating `MountRef`
/// rejects the envelope at deserialization — so the loudness the `overrides` floor buys with a check is
/// already free here, and this constant states the fact and nothing more.
pub const MIN_VERSION_FOR_ROUTER_MOUNT_REF: &str = "0.29.0";

/// This build's own version, as the acceptance ceiling — see [`NORMALIZED_AST_CONTRACT_VERSION`].
/// Every crate inherits the workspace version (`version.workspace = true`), so this is the number
/// `zzop version` prints.
pub const SUPPORTED_NORMALIZED_AST_VERSION: &str = env!("CARGO_PKG_VERSION");

/// `"0.27.0"` -> `(0, 27, 0)`, or `None` when the string is not three dot-separated integers.
///
/// Hand-rolled rather than a `semver` dependency: the envelope contract needs ordering over
/// `MAJOR.MINOR.PATCH` and nothing else — no pre-release tags, no build metadata, no ranges — and a
/// dependency whose extra semantics nobody uses is a surface that can disagree with this crate's own
/// idea of what a version is. Tuple comparison gives the ordering directly.
pub fn parse_contract_version(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.split('.');
    let mut next = || parts.next()?.parse::<u32>().ok();
    let parsed = (next()?, next()?, next()?);
    parts.next().is_none().then_some(parsed)
}
