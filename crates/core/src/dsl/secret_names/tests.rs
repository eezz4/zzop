use super::*;

/// The built-in NAMES and the built-in ALTERNATION have to be the same vocabulary, or `zzop init`
/// writes a config that changes the rule the moment it is honored.
///
/// Checked by BEHAVIOR rather than by string equality: the names expand `[_-]?` into three spellings,
/// so the two texts are deliberately different and only a matcher can say they agree.
#[test]
fn the_shipped_names_rebuild_the_built_in_alternation() {
    let names: Vec<String> = BUILT_IN_NAMES.iter().map(|s| s.to_string()).collect();
    let rebuilt = alternation_from(&names).expect("the built-in names are not empty");
    let built_in = regex::Regex::new(&format!("(?i)^(?:{BUILT_IN_ALTERNATION})$"))
        .expect("the built-in alternation compiles");
    let declared = regex::Regex::new(&format!("(?i)^(?:{rebuilt})$"))
        .expect("the rebuilt alternation compiles");

    for name in BUILT_IN_NAMES {
        assert!(
            built_in.is_match(name),
            "built-in rejects its own name {name}"
        );
        assert!(declared.is_match(name), "rebuilt rejects {name}");
    }
    // ...and neither accepts what the corpus measurement rejected. This is the standing negative from
    // `rules/dsl/security/secrets.rs` restated at the vocabulary layer, so a widening of the NAME list
    // fails here too and not only three crates away.
    for noise in ["key", "accesstoken", "apitoken", "auth", "signature", "jwt"] {
        assert!(!built_in.is_match(noise), "built-in accepts {noise}");
        assert!(!declared.is_match(noise), "rebuilt accepts {noise}");
    }
}

/// An undeclared or all-empty list asks for nothing, and says so as `None` rather than as an empty
/// alternation — which would be a regex matching the empty string, i.e. EVERY name.
#[test]
fn nothing_declared_is_none_and_never_an_empty_alternation() {
    for names in [vec![], vec![String::new()], vec!["  ".to_string()]] {
        assert_eq!(alternation_from(&names), None, "{names:?}");
    }
}

/// A declaration says WHICH names, never how the pattern behaves.
#[test]
fn a_declared_name_is_escaped_rather_than_interpreted() {
    let names = vec!["a.b".to_string(), "x|y".to_string(), "p(q".to_string()];
    let alt = alternation_from(&names).expect("non-empty");
    let re = regex::Regex::new(&format!("(?i)^(?:{alt})$")).expect("escaped names always compile");
    assert!(re.is_match("a.b"));
    assert!(!re.is_match("axb"), "`.` must be a literal dot: {alt}");
    assert!(re.is_match("x|y"), "`|` must be a literal pipe: {alt}");
    assert!(
        !re.is_match("x"),
        "`x|y` must not become two alternatives: {alt}"
    );
    assert!(re.is_match("p(q"));
}

/// Longest first, so the captured group is the longest name that fits rather than a prefix of it.
#[test]
fn longer_names_come_first_so_a_finding_reports_the_name_the_source_spells() {
    let names = vec!["secret".to_string(), "secret_key".to_string()];
    let alt = alternation_from(&names).expect("non-empty");
    assert!(
        alt.starts_with("secret_key"),
        "the longer name must come first: {alt}"
    );
    let re = regex::Regex::new(&format!("(?i)({alt})")).expect("compiles");
    assert_eq!(
        &re.captures("SECRET_KEY").expect("matches")[1],
        "SECRET_KEY"
    );
}
