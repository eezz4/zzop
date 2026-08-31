//! THE SUPPRESSION CENSUS — the `zzop-<rule>-ok` markers a tree carries.
//!
//! Its own module rather than a sixth resident of `config_filters`: a marker is not a config key. It is
//! an annotation the author wrote INTO the source, so it is neither declared in `zzop.config.jsonc` nor
//! discoverable by reading one — which is exactly why it needed a disclosure of its own.

/// Scope self-report: the `zzop-<rule>-ok` suppression markers this tree carries. `sites` is every
/// site in walk order; `None` when the tree has none, so an ordinary run stays quiet.
///
/// The last silence in the product with NO trace in the output: a suppressed finding is absent from
/// `findings` by definition, so `findings.total` reads one lower and nothing else moves, and a reviewer
/// handed the report cannot tell a clean file from a quieted one without grepping the tree themselves
/// (measured with a hardcoded production credential silenced by one comment — the whole reply carried
/// no key matching `/suppress/i`). Claims markers PRESENT, never "findings suppressed";
/// `zzop_core::dsl::suppress_marker_sites` owns why that is the honest count. Names the marker TOKEN at
/// each site, because the token is the rule.
pub(crate) fn suppressed_findings_warning(
    sites: &[zzop_core::dsl::SuppressMarkerSite],
) -> Option<String> {
    if sites.is_empty() {
        return None;
    }
    let mut rules: Vec<&str> = sites.iter().map(|s| s.marker.as_str()).collect();
    rules.sort_unstable();
    rules.dedup();
    let mut sample = (sites.iter())
        .take(super::SAMPLE)
        .map(|s| format!("{}:{} `{}`", s.file, s.line, s.marker))
        .collect::<Vec<String>>()
        .join(", ");
    if sites.len() > super::SAMPLE {
        sample.push_str(&format!(", +{} more", sites.len() - super::SAMPLE));
    }
    let plural = if sites.len() == 1 { "" } else { "s" };
    Some(format!(
        "{} suppression marker{plural} in this tree, spanning {} rule id(s): {sample}. A marker silences \
         its rule on that line and the line below it, so anything it silenced is NOT in `findings` and \
         no count here moves — this line is the only trace such a silence leaves. It counts markers \
         PRESENT, not findings suppressed: a marker left behind after the code was fixed is listed and \
         silenced nothing, and reading the count as \"N real findings were hidden\" over-reads it. To \
         see what a marker is holding back, delete it and re-run.",
        sites.len(),
        rules.len()
    ))
}
