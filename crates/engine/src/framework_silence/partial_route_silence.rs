//! S16: the PARTIAL-silence tripwire, and the gap its siblings structurally cannot see.
//!
//! Every provide-side tripwire before this one asks a TREE-WIDE question: are there almost no http
//! routes here at all? That catches total blindness and nothing else. Measured on a 14-route Express
//! tree where the extractor missed 5: the tree still had 9 routes, so every existing tripwire stayed
//! quiet — and the run did not go silent, it went CONFIDENTLY WRONG. Three "no matching provide"
//! warnings for routes that were right there in the source, a `graph --domain posture` census
//! reporting 4 mutating routes as the total when there were 7, an `endpoint` lookup answering
//! `not-found` for a route in the tree, and `coverage`'s `blindSpots: []`.
//!
//! Partial loss is also the COMMON case, which is what makes the gap matter: a framework is usually
//! recognized in its ordinary spelling and missed in one idiom, so most real extraction gaps land here
//! rather than at zero.
//!
//! ## The question this asks instead
//! Per FILE: this file imports a server framework and produced no http route. That is not a defect on
//! its own — a file can import `express` to define middleware, types or a test helper — which is why
//! this reports a per-file census rather than a verdict, and why it stays quiet when the tree-wide
//! tripwires are already firing (S1/S2 own the total case, and two warnings about the same silence
//! read as two problems).
//!
//! It is deliberately NOT the cross-check that first suggested it — a DSL rule quoting a snippet from
//! `app.get('/api/reports/monthly', …` in a file whose `io.provides` was empty. That evidence is real
//! but it only exists when some unrelated rule happens to match that line, so it would report the gap
//! on the trees where a lint rule fired and stay silent on the rest. The import is present either way.

use std::collections::{BTreeMap, BTreeSet};

use super::server_framework_import::is_server_framework_specifier;

/// Below this many silent framework-importing files, say nothing: one such file is the ordinary
/// middleware/types/helper case and reporting it would put a warning on healthy trees.
const MIN_SILENT_FILES: usize = 2;

/// Sample size for the named files — this repo's standard for path samples in a disclosure.
const SAMPLE: usize = 3;

/// Names files that import a server framework and contributed no `http` provide, when the tree DID
/// extract some. `None` when the tree extracted none at all (S1/S2 own that case and say it better),
/// when no framework-importing file is silent, or when too few are to be worth a line.
pub fn partial_route_silence_warning(
    package_import_files: &BTreeMap<String, BTreeSet<String>>,
    provide_files: &BTreeSet<String>,
    extracted_http_provides: usize,
) -> Option<String> {
    if extracted_http_provides == 0 {
        return None; // total silence — not this tripwire's question
    }
    let mut silent: BTreeSet<&str> = BTreeSet::new();
    let mut importing = 0usize;
    for (specifier, files) in package_import_files {
        if !is_server_framework_specifier(specifier) {
            continue;
        }
        for file in files {
            importing += 1;
            if !provide_files.contains(file) {
                silent.insert(file.as_str());
            }
        }
    }
    if silent.len() < MIN_SILENT_FILES {
        return None;
    }
    let sample: Vec<&str> = silent.iter().copied().take(SAMPLE).collect();
    let more = silent.len().saturating_sub(sample.len());
    let more = if more > 0 {
        format!(", and {more} more")
    } else {
        String::new()
    };
    Some(format!(
        "{} file(s) import a server framework but contributed NO http route, while {extracted_http_provides} \
         route(s) were extracted elsewhere in this tree ({} framework-importing file(s) in total): {}{more}. \
         Some of those are ordinary — a file can import a framework for middleware, types or a test \
         helper without registering anything. But this is the shape a PARTIAL extraction gap takes, and \
         partial is the common one: a framework recognized in its usual spelling and missed in one \
         idiom (a router received as a function parameter, a registration inside a struct method). \
         Every other coverage self-report here asks a tree-wide question and can only see a tree with \
         almost NO routes, so a tree missing a third of them looks healthy to all of them — and the \
         downstream answers do not go quiet, they go confidently wrong (a route that exists reported as \
         having no provider, a route census reporting what was SEEN as a total). If a listed file does \
         register routes, that idiom is not extracted here: project them with a Mode B overlay adapter, \
         or declare them with `trees[].routes` if routes are the only thing missing.",
        silent.len(),
        importing,
        sample.join(", "),
    ))
}

#[cfg(test)]
mod tests;
