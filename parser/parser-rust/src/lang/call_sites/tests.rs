use super::*;

fn sites(src: &str) -> Vec<(String, u32, String)> {
    extract_call_sites("f.rs", src)
        .into_iter()
        .map(|s| (s.kind, s.line, s.callee))
        .collect()
}

// --- env-read: both spellings, kept distinct ---

#[test]
fn std_env_var_and_var_os_emit_with_spelling_as_written() {
    let src = "fn f() {\n    let a = std::env::var(\"HOME\");\n    let b = std::env::var_os(\"PATH\");\n}\n";
    assert_eq!(
        sites(src),
        vec![
            ("env-read".into(), 2, "std::env::var".into()),
            ("env-read".into(), 3, "std::env::var_os".into()),
        ]
    );
}

#[test]
fn use_qualified_env_var_spelling_emits_its_own_spelling() {
    // Two spellings of one function stay two spellings — the channel's original-spelling contract.
    let src = "use std::env;\n\nfn f() {\n    let a = env::var(\"HOME\");\n    let b = env::var_os(\"PATH\");\n}\n";
    assert_eq!(
        sites(src),
        vec![
            ("env-read".into(), 4, "env::var".into()),
            ("env-read".into(), 5, "env::var_os".into()),
        ]
    );
}

#[test]
fn dynamic_key_still_emits() {
    let src =
        "fn f(name: &str) -> Result<String, std::env::VarError> {\n    std::env::var(name)\n}\n";
    assert_eq!(
        sites(src),
        vec![("env-read".into(), 2, "std::env::var".into())]
    );
}

// --- the compile-time boundary (the channel doc's own named exclusion) ---

#[test]
fn env_macro_and_option_env_are_deliberately_silent() {
    // env!()/option_env!() resolve at COMPILE time and read no process environment at run time.
    let src = "fn f() -> &'static str {\n    let _ = option_env!(\"OPT\");\n    env!(\"CARGO_PKG_VERSION\")\n}\n";
    assert_eq!(sites(src), vec![]);
}

// --- console-write is not produced (the println! judgment) ---

#[test]
fn println_family_emits_nothing_at_all() {
    // Fact-layer console writes, permanently unconsumed for `.rs` (module doc) — so not produced.
    let src = "fn f() {\n    println!(\"x\");\n    eprintln!(\"y\");\n    print!(\"z\");\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn log_and_tracing_macros_are_silent() {
    let src = "fn f() {\n    log::error!(\"x\");\n    tracing::info!(\"y\");\n}\n";
    assert_eq!(sites(src), vec![]);
}

// --- resolvability boundaries ---

#[test]
fn bare_var_after_use_is_silent() {
    // Spelled `var` at the site — a name far too common to claim (module doc).
    let src = "use std::env::var;\n\nfn f() {\n    let a = var(\"HOME\");\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn whole_environment_iteration_is_silent() {
    let src = "fn f() {\n    for (k, v) in std::env::vars() {\n        drop((k, v));\n    }\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn aliased_module_path_is_silent() {
    let src = "use std::env as e;\n\nfn f() {\n    let a = e::var(\"HOME\");\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn a_leading_colon_path_is_silent() {
    // `::std::env::var` is the crate-root-anchored FIFTH spelling of the same function — admitting
    // it means normalizing paths, which the original-spelling contract forbids (module doc's
    // deliberate-silences list, promoted there from an inline comment by review).
    let src = "fn f() -> Result<String, std::env::VarError> {\n    ::std::env::var(\"HOME\")\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn a_local_env_module_is_over_captured_and_disclosed() {
    // NOT a desired behavior — the pinned cost of the syntactic path check (module doc's "Known
    // imprecision, accepted"): a file-local `mod env` shadows `std::env` and its `var` is still
    // reported, because no name resolution happens. If this test ever goes red because the producer
    // learned resolution, delete it together with that doc section.
    let src = "mod env {\n    pub fn var(_k: &str) -> String {\n        String::new()\n    }\n}\n\nfn f() -> String {\n    env::var(\"HOME\")\n}\n";
    assert_eq!(sites(src), vec![("env-read".into(), 8, "env::var".into())]);
}

#[test]
fn a_user_env_var_function_on_another_type_is_silent() {
    let src = "fn f(cfg: &Config) {\n    let a = cfg.var(\"HOME\");\n    let b = my::env2::var(\"X\");\n}\n";
    assert_eq!(sites(src), vec![]);
}

// --- text boundaries and degrade ---

#[test]
fn string_and_comment_mentions_never_fire() {
    let src = "fn f() -> &'static str {\n    // std::env::var(\"HOME\")\n    \"std::env::var(name)\"\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn call_inside_a_macro_invocation_is_invisible_and_disclosed() {
    // syn parses macro arguments as an opaque TokenStream (module doc's macro scope note) — the
    // read inside `format!` is real and NOT seen; this pin is the disclosure.
    let src = "fn f() -> String {\n    format!(\"{}\", std::env::var(\"HOME\").unwrap())\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn unparseable_input_yields_empty() {
    assert_eq!(sites("fn broken( {{{"), vec![]);
}

// --- process-exec (wave 3) ---

#[test]
fn command_new_emits_in_each_of_its_three_spellings() {
    let src = "use std::process;\nuse std::process::Command;\n\nfn run(name: &str) {\n    std::process::Command::new(name);\n    process::Command::new(name);\n    Command::new(name);\n}\n";
    assert_eq!(
        sites(src),
        vec![
            (
                "process-exec".into(),
                5,
                "std::process::Command::new".into()
            ),
            ("process-exec".into(), 6, "process::Command::new".into()),
            ("process-exec".into(), 7, "Command::new".into()),
        ]
    );
}

#[test]
fn builder_methods_after_command_new_are_not_separate_sites() {
    let src = "use std::process::Command;\n\nfn run(name: &str) -> std::io::Result<std::process::Output> {\n    Command::new(\"sh\").arg(\"-c\").arg(name).output()\n}\n";
    assert_eq!(
        sites(src),
        vec![("process-exec".into(), 4, "Command::new".into())]
    );
}

#[test]
fn process_exit_and_third_party_runners_are_not_this_family() {
    // `exit`/`abort` end THIS process rather than launching one; `tokio::process` is not std's API
    // and the consuming rule's argv claim is stated about std's (module doc).
    let src = "fn run(name: &str) {\n    tokio::process::Command::new(name);\n    std::process::exit(1);\n}\n";
    assert_eq!(sites(src), vec![]);
}

// --- hash-call (wave 4): the one family where a third-party surface IS claimed ---

fn hash_sites(src: &str) -> Vec<(u32, String, Option<String>)> {
    extract_call_sites("f.rs", src)
        .into_iter()
        .filter(|s| s.kind == "hash-call")
        .map(|s| (s.line, s.callee, s.algorithm))
        .collect()
}

#[test]
fn crate_digest_constructors_carry_the_type_or_crate_as_the_algorithm() {
    let src = "use md5;
use sha1::Sha1;

fn h(b: &[u8]) {
    md5::compute(b);
    Sha1::new();
    sha2::Sha256::new();
}
";
    assert_eq!(
        hash_sites(src),
        vec![
            (5, "md5::compute".to_string(), Some("md5".to_string())),
            (6, "Sha1::new".to_string(), Some("Sha1".to_string())),
            (
                7,
                "sha2::Sha256::new".to_string(),
                Some("Sha256".to_string())
            ),
        ]
    );
}

#[test]
fn adaptive_hashes_kdfs_and_unclaimed_crates_are_silent() {
    // `bcrypt` is the RECOMMENDED answer rather than the defect; `ring`/`openssl` select their digest
    // through an algorithm CONSTANT passed as a value, a shape this channel does not resolve.
    let src = "fn h(pw: &str, b: &[u8]) {
    bcrypt::hash(pw, 12);
    ring::digest::digest(&ring::digest::SHA256, b);
    hmac::Hmac::<sha2::Sha256>::new_from_slice(b);
}
";
    assert_eq!(hash_sites(src), vec![]);
}

#[test]
fn a_digest_named_only_in_a_string_or_comment_is_not_a_site() {
    let src = "fn doc() -> &'static str {
    // md5::compute(b)
    \"md5::compute(b)\"
}
";
    assert_eq!(hash_sites(src), vec![]);
}
