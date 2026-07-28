//! The auth-acquisition path vocabularies and the one helper that compiles a caller-declared vocabulary
//! pattern. Split out of the parent module purely to keep that file under the repo's per-file line cap.
//!
//! Two tiers, because some acquisition-shaped words also legitimately name unrelated mutating routes
//! (`POST /devices/register`):
//! - **Standalone** ([`AUTH_ACQUISITION_STANDALONE_PATTERN`]): exempt unconditionally — these segments ARE
//!   the auth surface regardless of what else is in the path.
//! - **Conditional** ([`AUTH_ACQUISITION_CONDITIONAL_PATTERN`]): exempt only when an auth-family segment
//!   ([`AUTH_FAMILY_PATH_PATTERN`]) also appears in the same path — `/auth/register` is exempt,
//!   `/devices/register` is not.
//!
//! Every list is matched `/`-delimited on whole path segments, never as a bare substring, so
//! `/author/profile` does not match `auth`.
//!
//! These are `pub` because they are the DEFAULTS behind declarable config keys
//! (`vocabulary.authAcquisition*`, `vocabulary.authFamilyPathPattern`): `zzop_engine::VocabularyConfig`
//! references these symbols to assemble what a config-less run assumes, instead of restating the values.

use regex::Regex;

/// Auth-acquisition exemption, standalone tier — see parent module doc "Auth-acquisition exemption".
pub const AUTH_ACQUISITION_STANDALONE_PATTERN: &str =
    r"(?i)/(auth|login|logout|signin|signup)(/|$)";

/// Auth-acquisition exemption, conditional tier — exempt only alongside [`AUTH_FAMILY_PATH_PATTERN`].
pub const AUTH_ACQUISITION_CONDITIONAL_PATTERN: &str =
    r"(?i)/(register|token|refresh|password|otp)(/|$)";

/// Auth-family gate for the conditional exemption tier — see parent module doc.
pub const AUTH_FAMILY_PATH_PATTERN: &str = r"(?i)/(auth|login|signin|signup|session|oauth)(/|$)";

/// Compiles a caller-declared vocabulary pattern, or `None` when there is nothing to compile: the author
/// declared none, or declared one that will not parse. The two are the same outcome on purpose — a
/// pattern we substitute for the author's is the guessing this vocabulary exists to remove, and neither
/// case may take the whole run down. `None` matches nothing, which for every caller here means the
/// exemption it gates is simply not granted (the direction that under-clears rather than over-clears).
pub(super) fn vocab_re(declared: Option<&str>) -> Option<Regex> {
    declared.and_then(|d| Regex::new(d).ok())
}

/// The compiled auth-acquisition surface for one run — the three tier patterns plus the tier rule that
/// combines them. Compiled once per call and asked per route. An undeclared tier exempts nothing.
pub(super) struct AcquisitionSurface {
    standalone: Option<Regex>,
    conditional: Option<Regex>,
    family: Option<Regex>,
}

fn matches(re: &Option<Regex>, path: &str) -> bool {
    re.as_ref().is_some_and(|r| r.is_match(path))
}

impl AcquisitionSurface {
    pub(super) fn compile(input: &super::ScanMutatingRouteNoAuthInput) -> Self {
        AcquisitionSurface {
            standalone: vocab_re(input.auth_acquisition_standalone_pattern),
            conditional: vocab_re(input.auth_acquisition_conditional_pattern),
            family: vocab_re(input.auth_family_path_pattern),
        }
    }

    /// Whether this path is auth-acquisition surface, and so exempt before the BFS ever runs.
    pub(super) fn exempts(&self, path: &str) -> bool {
        matches(&self.standalone, path)
            || (matches(&self.conditional, path) && matches(&self.family, path))
    }
}
