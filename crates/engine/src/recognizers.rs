//! The engine aggregator half of the framework-recognizer mechanism (`zzop_core::recognizer`'s module
//! doc holds the contract): each parser crate declares, next to its own adapters, which frameworks it
//! can recognize, and this module composes those declarations into the one list a consumer reads — the
//! same shape [`crate::rule_sightlines`] gives the per-rule half, and for the same reason: the engine
//! may enumerate, never own, per-parser data.
//!
//! The list is CAPABILITY-kind ("this build can/cannot see X"), independent of any run. That is
//! exactly what the existing disclosure could not be: every `framework_silence` tripwire (S1-S8) is
//! per-run and fires only on a tree ALREADY showing the symptom, so *"does this tool know my stack?"*
//! had no answer before the first run — the open half of `2.backlog/hard.md` H1.
//!
//! # Why the aggregate is worth more than any one declaration
//! Grouped by `emits`, this list makes a shape visible that no per-parser reading shows: a language can
//! carry several recognizers and still fill only ONE side of the cross-layer join. Measured 2026-08-01,
//! `parser-java-21` declared two provide-side recognizers and zero consume-side ones — a Java service
//! that calls another service contributed nothing to the join — while `parser-rust` covered BOTH sides
//! with the same two rows. That very asymmetry is what this grouping surfaced, and it closed on
//! 2026-08-02 (`resttemplate`/`webclient` consume rows landed, plus a `jpa` db row); the point survives
//! the closure: recognizer COUNT ranked those two parsers the wrong way round the whole time, the
//! channel grouping did not, which is why `emits` is a field rather than prose.

use zzop_core::FrameworkRecognizer;

/// Every framework recognizer compiled into this build, parser crate by parser crate.
///
/// Order is the parser crates' own declaration order within a stable crate sequence, so the output is
/// deterministic without a sort — and deliberately NOT sorted by framework name, because grouping by
/// the owning parser is what makes a missing channel legible next to its siblings.
pub fn framework_recognizers() -> Vec<FrameworkRecognizer> {
    [
        zzop_parser_typescript::FRAMEWORK_RECOGNIZERS,
        zzop_parser_python_3::FRAMEWORK_RECOGNIZERS,
        zzop_parser_java_21::FRAMEWORK_RECOGNIZERS,
        zzop_parser_csharp::FRAMEWORK_RECOGNIZERS,
        zzop_parser_go::FRAMEWORK_RECOGNIZERS,
        zzop_parser_rust::FRAMEWORK_RECOGNIZERS,
        zzop_parser_prisma::FRAMEWORK_RECOGNIZERS,
        zzop_parser_sql::FRAMEWORK_RECOGNIZERS,
    ]
    .concat()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Every parser crate must contribute at least one row. An empty contribution is the failure this
    /// whole mechanism exists to abolish — it would read as "this build has no recognizer for that
    /// language" when it actually means "nobody declared", and those are the two states this repo has
    /// paid for confusing more than once.
    #[test]
    fn every_parser_crate_contributes_at_least_one_declaration() {
        let lists = [
            ("typescript", zzop_parser_typescript::FRAMEWORK_RECOGNIZERS),
            ("python-3", zzop_parser_python_3::FRAMEWORK_RECOGNIZERS),
            ("java-21", zzop_parser_java_21::FRAMEWORK_RECOGNIZERS),
            ("csharp", zzop_parser_csharp::FRAMEWORK_RECOGNIZERS),
            ("go", zzop_parser_go::FRAMEWORK_RECOGNIZERS),
            ("rust", zzop_parser_rust::FRAMEWORK_RECOGNIZERS),
            ("prisma", zzop_parser_prisma::FRAMEWORK_RECOGNIZERS),
            ("sql", zzop_parser_sql::FRAMEWORK_RECOGNIZERS),
        ];
        for (name, list) in lists {
            assert!(
                !list.is_empty(),
                "parser-{name} declares no framework recognizer — a declaration format still declares \
                 ITSELF (see parser-prisma), so an empty list is always an undeclared parser"
            );
        }
        // ... and the aggregator must actually carry all of them, not silently drop a crate.
        let total: usize = lists.iter().map(|(_, l)| l.len()).sum();
        assert_eq!(
            framework_recognizers().len(),
            total,
            "the aggregator dropped a crate's declarations"
        );
    }

    /// The channel vocabulary is closed. A typo'd `emits` string would put a recognizer in a bucket
    /// that no consumer groups by, and it would look like an absent channel — the same silent shape as
    /// not declaring at all.
    #[test]
    fn every_declared_channel_is_one_of_the_named_constants() {
        use zzop_core::recognizer::channel;
        let allowed: BTreeSet<&str> = [
            channel::PROVIDES,
            channel::CONSUMES,
            channel::DB,
            channel::AUTH_EVIDENCE,
        ]
        .into_iter()
        .collect();
        for r in framework_recognizers() {
            assert!(!r.emits.is_empty(), "{} declares no channel", r.framework);
            for e in r.emits {
                assert!(
                    allowed.contains(e),
                    "{} declares unknown channel {e:?} — use zzop_core::recognizer::channel::*",
                    r.framework
                );
            }
            assert!(
                !r.extensions.is_empty(),
                "{} declares no extension, so no tree could ever match it",
                r.framework
            );
            for ext in r.extensions {
                assert!(
                    !ext.starts_with('.') && ext.to_ascii_lowercase() == **ext,
                    "{}: extension {ext:?} must be lowercase and dot-free, as the coverage surface \
                     groups files",
                    r.framework
                );
            }
        }
    }

    // `java_has_no_consume_side_recognizer_and_says_so` lived here until 2026-08-02, pinning the
    // "java fills half a join" asymmetry AND its own deletion condition: "when a
    // Feign/RestTemplate/WebClient recognizer lands, this test fails and is deleted along with the
    // note in that crate's declaration". The `resttemplate`/`webclient` consume rows landed, so it
    // was deleted exactly as it directed — the channel binding (`rule_contracts::recognizer_channels`)
    // is what now holds those rows to their code.
}
