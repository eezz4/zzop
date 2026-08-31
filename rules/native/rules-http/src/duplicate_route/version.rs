//! Version-scope disclosure for `duplicate-route` — the SECOND non-erasing axis, built to the shape
//! `boundary` already proved (`route-version-v1`, 2026-08-21).
//!
//! ## What the rule was getting wrong
//! The group key is the normalized `"METHOD /path"` and nothing else. That is correct for every
//! framework that puts its version in the URL, and blind to every framework that does not. NestJS's
//! `VersioningType.HEADER`/`CUSTOM` is the second kind: cal.com registers `cal-api-version` as a
//! CUSTOM version extractor and splits one path across `@Controller({path, version})` classes, so
//! `/v2/bookings`, `/v2/event-types` and `/v2/schedules` each read as a collision between controllers
//! that can never receive the same request. Measured 2026-08-21 (`analyze --rule duplicate-route`):
//! 15 of 15 cal.com findings were exactly this, from three controller pairs whose version sets are
//! pairwise disjoint.
//!
//! Noise is the smaller half. The prescription was destructive: deleting
//! `BookingsController_2024_04_15` breaks every customer pinned to that version, and "merge the
//! handlers" is not even expressible, because the two controllers take different input DTOs BY
//! DESIGN. A reader who followed the message did damage; a reader who did not learned to distrust the
//! rule. So the message's lead changes on this axis, not just its severity — see the rule body.
//!
//! ## Why the manifest axis could not reach it
//! All 30 of those sites live under `apps/api/v2/src/`, and `apps/api/v2/package.json` is the nearest
//! deployment manifest for every one of them. Same boundary on both sides means
//! `boundary::disclosure` correctly says nothing, and all 15 findings carried no
//! `manifestBoundaries` and stayed at `warning`. The version axis is the only thing that reaches
//! them, which is why it is a second axis rather than a widening of the first.
//!
//! ## What a difference proves, and what it does not
//! Two DIFFERENT version texts prove the two sites declare different version SCOPES. They do not
//! prove the scopes are disjoint, and this module never claims they are. `route_version` is the
//! source expression, not a resolved set — `API_VERSIONS_VALUES` and `VERSION_2024_08_13_VALUE` are
//! two texts that could perfectly well overlap (no such pair collides in cal.com today, but nothing
//! here rules it out). So the axis does what `boundary` does and no more: it appends a sentence, adds
//! a machine-readable field, and moves the finding out of the `--fail-on warning` gate. It never
//! drops one. The standing asymmetry is the reason (`.claude` §24): a finding raised on a heuristic
//! is dismissed in seconds, a finding DELETED by one is a shadow that leaves no trace.
//!
//! Absence is "not measured", never "same version". Only the native TypeScript controller extractor
//! emits this field today; every other producer emits `None`, and a one-sided version therefore keeps
//! the warning — the same rule `boundary` applies to an unmeasured manifest, for the same reason. An
//! IDENTICAL version on both sides discloses nothing either, and that direction is not a technicality:
//! two controllers claiming the same version at one path is a real ambiguity, and it is the shape a
//! duplicate INSIDE one versioned controller has.

/// The sentence appended to the message when the two sites declare different version scopes, and the
/// `data` payload carrying the same fact machine-readably. `None` when the versions match, when either
/// side has none, or when neither does — in each of those there is nothing established to disclose.
pub(super) fn disclosure(
    first: &zzop_core::IoProvide,
    dup: &zzop_core::IoProvide,
) -> Option<(String, serde_json::Value)> {
    let a = first.route_version.as_deref()?;
    let b = dup.route_version.as_deref()?;
    if a == b {
        return None;
    }
    let sentence = format!(
        " The two sites declare DIFFERENT version scopes — `{a}` and `{b}` (the `version` each \
         controller declared, carried verbatim as the source spells it). THAT IS WHY THIS FINDING IS \
         REPORTED AT `info` rather than `warning`: a framework can version by something the URL never \
         carries — a request header or a media type, as NestJS's `VersioningType.HEADER`/`CUSTOM` \
         does — and then two handlers at one path are a deliberate versioned split that no request \
         can reach at once, not a shadow. DO NOT merge or delete either handler on the strength of \
         this finding: their request and response contracts differ by design, which is what a version \
         split is FOR, and removing the older one breaks every client pinned to it. The finding is NOT \
         cleared, because zzop cannot prove the two scopes are DISJOINT — it reads the version \
         EXPRESSION, not its value, and two different spellings can still name overlapping sets (a \
         catch-all constant beside a single-version one is exactly that shape). Confirm they do not \
         overlap, and this pair is resolved. A site with NO version named here was not measured — most \
         producers never emit one — and an unmeasured side is never reported as differing, so it keeps \
         the warning."
    );
    let data = serde_json::json!({ "first": a, "duplicate": b });
    Some((sentence, data))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provide(file: &str, version: Option<&str>) -> zzop_core::IoProvide {
        zzop_core::IoProvide {
            route_version: version.map(str::to_string),
            response: None,
            body: None,
            kind: "http".to_string(),
            key: "GET /v2/bookings".to_string(),
            file: file.to_string(),
            line: 1,
            symbol: None,
        }
    }

    #[test]
    fn two_different_version_texts_disclose_both() {
        let (sentence, data) = disclosure(
            &provide("a.ts", Some("API_VERSIONS_VALUES")),
            &provide("b.ts", Some("VERSION_2024_08_13_VALUE")),
        )
        .expect("differs");
        assert!(sentence.contains("API_VERSIONS_VALUES"));
        assert!(sentence.contains("VERSION_2024_08_13_VALUE"));
        assert!(sentence.contains("NOT cleared"), "{sentence}");
        assert_eq!(data["first"], "API_VERSIONS_VALUES");
        assert_eq!(data["duplicate"], "VERSION_2024_08_13_VALUE");
    }

    #[test]
    fn an_identical_version_discloses_nothing() {
        assert!(disclosure(&provide("a.ts", Some("V1")), &provide("b.ts", Some("V1"))).is_none());
    }

    /// An unmeasured side is not a difference. Both directions, because a producer that emits the
    /// field and one that does not can sit on either end of the pair.
    #[test]
    fn an_unmeasured_side_discloses_nothing() {
        assert!(disclosure(&provide("a.ts", Some("V1")), &provide("b.ts", None)).is_none());
        assert!(disclosure(&provide("a.ts", None), &provide("b.ts", Some("V1"))).is_none());
        assert!(disclosure(&provide("a.ts", None), &provide("b.ts", None)).is_none());
    }
}
