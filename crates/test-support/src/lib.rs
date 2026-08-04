//! Shared test-only helpers. A DEV dependency of the crates that use it and of nothing else — no
//! shipped binary links this crate.
//!
//! It exists for one reason: a test that skips itself has to SAY SO, and stderr is the only channel
//! it has. `cargo test` gives a test no logger and no warning stream; a silent `return` is
//! indistinguishable from a pass, so a machine without `git` on PATH would report a green run over
//! tests that never executed. The message is addressed to the human who ran `cargo test`.
//!
//! Before this crate the notice was 33 hand-written `eprintln!` calls across six files. They could
//! not be covered the way every other stderr writer in this workspace is covered — by a crate-level
//! `#![allow]` at an entry point that prints by design — because an integration test IS its own
//! crate root, so there is no shared root to put the attribute on, and because two of the six files
//! are `#[cfg(test)] mod`s inside a shipped library, where a crate-level allow would punch a hole in
//! exactly the lib the policy protects. Folding them here leaves ONE stderr writer in the
//! workspace's test surface, with one exemption, in a crate that ships nowhere.

/// Emits `skipping <test>: <reason>` on stderr. Call it through [`skip_notice!`], which supplies
/// `site_marker` — do not name it directly.
#[doc(hidden)]
#[allow(
    clippy::print_stderr,
    reason = "The one deliberate stderr writer in the test surface, and the reason this crate \
              exists. A skipped test has no other way to tell the person who ran `cargo test` that \
              it did not run: nothing reads a test's stdout by default and there is no warning \
              channel. Every other stderr site in the workspace is either a binary entry point \
              covered by a crate-level #![allow] or a violation. This one is neither, so it is \
              named here, once."
)]
pub fn emit_skip_notice<Site>(site_marker: Site, reason: core::fmt::Arguments<'_>) {
    eprintln!(
        "{}",
        skip_notice_line(enclosing_test_name(site_marker), reason)
    );
}

/// The notice's exact text, split out from the printing so it can be asserted. `emit_skip_notice`
/// cannot be tested directly — `eprintln!` writes to the process's real stderr, which a unit test
/// cannot capture — and the wording IS the contract here: it is what tells a human that a green run
/// contained tests that never executed.
fn skip_notice_line(test: &str, reason: core::fmt::Arguments<'_>) -> String {
    format!("skipping {test}: {reason}")
}

/// The suffix [`skip_notice!`] plants in the caller's body so the enclosing function can be named.
#[doc(hidden)]
pub const SITE_MARKER_SUFFIX: &str = "::__zzop_skip_notice_site";

/// Recovers the enclosing test function's name from the marker item's type path.
///
/// The name is DERIVED rather than passed in because the hand-written form it replaces repeated the
/// test's own name inside its body — a string that silently stops matching the moment the test is
/// renamed.
///
/// `Site` is the zero-sized fn item [`skip_notice!`] defines inside the caller, so its type name is
/// the caller's own path with [`SITE_MARKER_SUFFIX`] appended, e.g.
/// `analyze_git::two_runs_are_deterministic::__zzop_skip_notice_site`. `type_name`'s output shape is
/// not a stability guarantee, so `self_names_the_enclosing_test` below asserts the derivation still
/// lands; a toolchain that changed the shape fails there rather than quietly printing marker names
/// into every skip notice.
fn enclosing_test_name<Site>(_site_marker: Site) -> &'static str {
    let path = core::any::type_name::<Site>();
    let path = path.strip_suffix(SITE_MARKER_SUFFIX).unwrap_or(path);
    path.rsplit("::").next().unwrap_or(path)
}

/// Announces that the enclosing test is skipping itself, and why.
///
/// ```ignore
/// if !git_available() {
///     zzop_test_support::skip_notice!("git not on PATH");
///     return;
/// }
/// ```
///
/// Prints `skipping <enclosing test fn>: git not on PATH`. Takes `format!`-style arguments. It does
/// NOT return from the caller — the `return` stays visible at the call site, because a macro that
/// hides control flow costs the next reader more than it saves.
#[macro_export]
macro_rules! skip_notice {
    ($($reason:tt)+) => {{
        // A fn item, not a closure: a closure's type name ends in `{{closure}}`, which names nothing.
        fn __zzop_skip_notice_site() {}
        $crate::emit_skip_notice(__zzop_skip_notice_site, ::core::format_args!($($reason)+))
    }};
}

#[cfg(test)]
mod tests {
    /// Pins the `type_name` derivation: this test's own name must come back out of the marker.
    /// The marker is planted inside a nested block, because that is the shape every real call site
    /// has (`if !git_available() { .. }`) and block nesting is the part of a definition path most
    /// likely to differ between toolchains.
    #[test]
    fn self_names_the_enclosing_test() {
        let derived = {
            fn __zzop_skip_notice_site() {}
            super::enclosing_test_name(__zzop_skip_notice_site)
        };
        assert_eq!(derived, "self_names_the_enclosing_test");
    }

    /// The full line, byte for byte. This is the wording the 33 folded `eprintln!` calls emitted, and
    /// keeping it identical is the difference between folding those calls and deleting them.
    #[test]
    fn the_line_reads_exactly_as_the_hand_written_form_did() {
        let line = {
            fn __zzop_skip_notice_site() {}
            super::skip_notice_line(
                super::enclosing_test_name(__zzop_skip_notice_site),
                format_args!("git not on PATH"),
            )
        };
        assert_eq!(
            line,
            "skipping the_line_reads_exactly_as_the_hand_written_form_did: git not on PATH"
        );
    }
}
