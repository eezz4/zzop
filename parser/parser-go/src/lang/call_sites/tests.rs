use super::*;

fn sites(src: &str) -> Vec<(String, u32, String)> {
    extract_call_sites("f.go", src)
        .into_iter()
        .map(|s| (s.kind, s.line, s.callee))
        .collect()
}

// --- console-write: the fmt.Print* trio ---

#[test]
fn fmt_print_family_emits_console_write_with_spelling_as_written() {
    let src = "package main\n\nimport \"fmt\"\n\nfunc f() {\n\tfmt.Println(\"a\")\n\tfmt.Print(\"b\")\n\tfmt.Printf(\"%d\", 1)\n}\n";
    assert_eq!(
        sites(src),
        vec![
            ("console-write".into(), 6, "fmt.Println".into()),
            ("console-write".into(), 7, "fmt.Print".into()),
            ("console-write".into(), 8, "fmt.Printf".into()),
        ]
    );
}

#[test]
fn fmt_sprint_family_is_silent() {
    // Builds a string, writes nothing — module doc's family line.
    let src =
        "package main\n\nimport \"fmt\"\n\nfunc f() string {\n\treturn fmt.Sprintf(\"%d\", 1)\n}\n";
    assert_eq!(sites(src), vec![]);
}

// --- console-write: Fprint* gated on the std-stream spelling ---

#[test]
fn fmt_fprint_to_spelled_std_streams_emits_but_other_writers_do_not() {
    let src = "package main\n\nimport (\n\t\"fmt\"\n\t\"os\"\n)\n\nfunc f(w Writer) {\n\tfmt.Fprintln(os.Stderr, \"x\")\n\tfmt.Fprintf(os.Stdout, \"%d\", 1)\n\tfmt.Fprintln(w, \"not console\")\n}\n";
    assert_eq!(
        sites(src),
        vec![
            ("console-write".into(), 9, "fmt.Fprintln".into()),
            ("console-write".into(), 10, "fmt.Fprintf".into()),
        ]
    );
}

#[test]
fn fmt_fprint_through_an_aliased_stream_is_silent() {
    // `w := os.Stdout` — the site spells `w`, and the check is the spelling at the site, never a
    // data-flow proof (module doc: degrade to silence, recall direction).
    let src = "package main\n\nimport (\n\t\"fmt\"\n\t\"os\"\n)\n\nfunc f() {\n\tw := os.Stdout\n\tfmt.Fprintln(w, \"x\")\n}\n";
    assert_eq!(sites(src), vec![]);
}

// --- the log.Print* exclusion (the design doc's named boundary case) ---

#[test]
fn log_print_family_is_deliberately_silent() {
    let src = "package main\n\nimport \"log\"\n\nfunc f() {\n\tlog.Println(\"x\")\n\tlog.Printf(\"%d\", 1)\n\tlog.Print(\"y\")\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn structured_loggers_are_silent() {
    let src = "package main\n\nfunc f(logger *Logger) {\n\tlogger.Info(\"x\")\n\tzap.L().Info(\"y\")\n}\n";
    assert_eq!(sites(src), vec![]);
}

// --- env-read ---

#[test]
fn os_getenv_and_lookupenv_emit_env_read() {
    let src = "package main\n\nimport \"os\"\n\nfunc f() {\n\t_ = os.Getenv(\"HOME\")\n\tv, ok := os.LookupEnv(\"PORT\")\n\t_ = v\n\t_ = ok\n}\n";
    assert_eq!(
        sites(src),
        vec![
            ("env-read".into(), 6, "os.Getenv".into()),
            ("env-read".into(), 7, "os.LookupEnv".into()),
        ]
    );
}

#[test]
fn dynamic_key_still_emits_env_read() {
    // The read point is statically witnessed; only the key would be a guess, and the key is not a
    // field — same population line as TS `process.env[k]` / Python `os.environ[k]`.
    let src = "package main\n\nimport \"os\"\n\nfunc f(name string) string {\n\treturn os.Getenv(name)\n}\n";
    assert_eq!(sites(src), vec![("env-read".into(), 6, "os.Getenv".into())]);
}

#[test]
fn os_environ_bulk_read_is_silent() {
    let src = "package main\n\nimport \"os\"\n\nfunc f() []string {\n\treturn os.Environ()\n}\n";
    assert_eq!(sites(src), vec![]);
}

// --- resolvability and text boundaries ---

#[test]
fn aliased_import_is_silent() {
    // `import f "fmt"` rebinds the package name — the selector at the site is not a spelling this
    // module names (module doc's aliased-import bullet).
    let src = "package main\n\nimport f \"fmt\"\n\nfunc g() {\n\tf.Println(\"x\")\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn string_and_comment_mentions_never_fire() {
    // The point of projecting instead of regexing.
    let src = "package main\n\nfunc f() string {\n\t// fmt.Println(\"in a comment\")\n\treturn \"fmt.Println(os.Getenv(a))\"\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn call_inside_another_calls_arguments_emits_outer_first() {
    // Preorder: the enclosing call's line is its own leftmost token's, so outer-before-inner holds
    // even on one line — and both calls are real sites.
    let src = "package main\n\nimport (\n\t\"fmt\"\n\t\"os\"\n)\n\nfunc f() {\n\tfmt.Println(os.Getenv(\"HOME\"))\n}\n";
    assert_eq!(
        sites(src),
        vec![
            ("console-write".into(), 9, "fmt.Println".into()),
            ("env-read".into(), 9, "os.Getenv".into()),
        ]
    );
}

#[test]
fn unparseable_input_yields_empty() {
    assert_eq!(sites("not go at all {{{{"), vec![]);
}

// --- process-exec (wave 3) ---

#[test]
fn os_exec_constructors_emit_process_exec() {
    let src = "package p\n\nimport (\n\t\"context\"\n\t\"os/exec\"\n)\n\nfunc run(ctx context.Context, name string) {\n\texec.Command(\"sh\", \"-c\", name)\n\texec.CommandContext(ctx, name)\n}\n";
    assert_eq!(
        sites(src),
        vec![
            ("process-exec".into(), 9, "exec.Command".into()),
            ("process-exec".into(), 10, "exec.CommandContext".into()),
        ]
    );
}

#[test]
fn the_cmd_builder_methods_are_not_separate_sites() {
    // One construction is one process (module doc) — `.Run()`'s receiver is a variable this producer
    // does not resolve, and emitting it would double-count the same fact.
    let src = "package p\n\nimport \"os/exec\"\n\nfunc run(name string) error {\n\tc := exec.Command(name)\n\treturn c.Run()\n}\n";
    assert_eq!(
        sites(src),
        vec![("process-exec".into(), 6, "exec.Command".into())]
    );
}

#[test]
fn syscall_exec_is_not_this_family() {
    let src = "package p\n\nimport \"syscall\"\n\nfunc run(name string) {\n\t_ = syscall.Exec(name, nil, nil)\n}\n";
    assert_eq!(sites(src), vec![]);
}

// --- hash-call (wave 4): the package IS the algorithm, so no argument is read ---

fn hash_sites(src: &str) -> Vec<(u32, String, Option<String>)> {
    extract_call_sites("f.go", src)
        .into_iter()
        .filter(|s| s.kind == "hash-call")
        .map(|s| (s.line, s.callee, s.algorithm))
        .collect()
}

#[test]
fn crypto_digest_constructors_carry_the_package_as_the_algorithm() {
    let src = "package p

import (
	\"crypto/md5\"
	\"crypto/sha1\"
	\"crypto/sha256\"
)

func h(b []byte) {
	md5.New()
	sha1.Sum(b)
	sha256.New()
}
";
    assert_eq!(
        hash_sites(src),
        vec![
            (10, "md5.New".to_string(), Some("md5".to_string())),
            (11, "sha1.Sum".to_string(), Some("sha1".to_string())),
            (12, "sha256.New".to_string(), Some("sha256".to_string())),
        ]
    );
}

#[test]
fn an_aliased_hash_import_is_silent() {
    // `import h "crypto/md5"` spells `h.New` — not a recognized spelling, the same line every other
    // family here draws for an alias.
    let src = "package p

import h \"crypto/md5\"

func f() {
	h.New()
}
";
    assert_eq!(hash_sites(src), vec![]);
}

#[test]
fn hmac_and_non_constructor_members_are_silent() {
    let src = "package p

import (
	\"crypto/hmac\"
	\"crypto/md5\"
)

func f(b []byte) {
	hmac.New(md5.New, b)
	_ = md5.Size
}
";
    // `md5.New` passed as a VALUE (not called) is not a call expression, so it emits nothing; the
    // enclosing `hmac.New` is not this family.
    assert_eq!(hash_sites(src), vec![]);
}
