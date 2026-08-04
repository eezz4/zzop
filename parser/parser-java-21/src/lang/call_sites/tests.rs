use super::*;

fn sites(src: &str) -> Vec<(String, u32, String)> {
    extract_call_sites("A.java", src)
        .into_iter()
        .map(|s| (s.kind, s.line, s.callee))
        .collect()
}

// --- console-write ---

#[test]
fn system_out_and_err_write_methods_emit_with_spelling_as_written() {
    let src = "class A {\n  void f() {\n    System.out.println(\"a\");\n    System.err.print(\"b\");\n    System.out.printf(\"%d\", 1);\n    System.err.format(\"%d\", 2);\n  }\n}\n";
    assert_eq!(
        sites(src),
        vec![
            ("console-write".into(), 3, "System.out.println".into()),
            ("console-write".into(), 4, "System.err.print".into()),
            ("console-write".into(), 5, "System.out.printf".into()),
            ("console-write".into(), 6, "System.err.format".into()),
        ]
    );
}

#[test]
fn slf4j_style_loggers_are_deliberately_silent() {
    // The FALSE-FOLD boundary the channel doc owns: configured output with levels and sinks is not a
    // console write, and a rule banning console writes is not banning logging.
    let src = "class A {\n  private static final Logger log = LoggerFactory.getLogger(A.class);\n  void f() {\n    log.info(\"x\");\n    log.error(\"y\", e);\n    logger.warn(\"z\");\n  }\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn aliased_stream_is_silent() {
    // `ps` is a data-flow alias of System.out; the check is the spelling at the site (module doc).
    let src = "class A {\n  void f() {\n    java.io.PrintStream ps = System.out;\n    ps.println(\"x\");\n  }\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn non_write_print_stream_methods_are_silent() {
    let src =
        "class A {\n  void f() {\n    System.out.flush();\n    System.out.append('c');\n  }\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn a_static_imported_out_is_silent() {
    // `import static java.lang.System.out;` — the site spells `out.println`, which does not name
    // `System`; the producer claims spellings, not bindings (module doc's static-import bullet, the
    // C# `using static` twin).
    let src = "import static java.lang.System.out;\n\nclass A {\n  void f() {\n    out.println(\"x\");\n  }\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn a_call_chained_onto_a_writes_result_emits_only_the_inner_spelling() {
    // `printf` returns the PrintStream, so the outer `.println` also writes — but its receiver is a
    // method invocation, not the `System.<stream>` field spelling, so only the inner call is a site
    // (module doc's chained-call bullet): one site per chain, on the spelling that names the stream.
    let src = "class A {\n  void f() {\n    System.out.printf(\"a\").println(\"b\");\n  }\n}\n";
    assert_eq!(
        sites(src),
        vec![("console-write".into(), 3, "System.out.printf".into())]
    );
}

#[test]
fn a_user_println_on_another_receiver_is_silent() {
    let src = "class A {\n  void f(Writer w) {\n    w.println(\"x\");\n    this.println(\"y\");\n  }\n  void println(String s) {}\n}\n";
    assert_eq!(sites(src), vec![]);
}

// --- env-read ---

#[test]
fn system_getenv_keyed_and_whole_map_both_emit() {
    // Both are real reads of the process environment; the channel carries no argument facts, so the
    // producer does not ask which form (module doc).
    let src = "class A {\n  void f() {\n    String p = System.getenv(\"PORT\");\n    java.util.Map<String,String> all = System.getenv();\n  }\n}\n";
    assert_eq!(
        sites(src),
        vec![
            ("env-read".into(), 3, "System.getenv".into()),
            ("env-read".into(), 4, "System.getenv".into()),
        ]
    );
}

#[test]
fn getenv_on_another_receiver_is_silent() {
    let src = "class A {\n  void f(Env env) {\n    env.getenv(\"PORT\");\n  }\n}\n";
    assert_eq!(sites(src), vec![]);
}

// --- text boundaries and degrade ---

#[test]
fn string_and_comment_mentions_never_fire() {
    let src = "class A {\n  // System.out.println(\"comment\")\n  String f() {\n    return \"System.getenv(x); System.out.println(y)\";\n  }\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn call_nested_in_an_argument_emits_both_outer_first() {
    let src = "class A {\n  void f() {\n    System.out.println(System.getenv(\"HOME\"));\n  }\n}\n";
    assert_eq!(
        sites(src),
        vec![
            ("console-write".into(), 3, "System.out.println".into()),
            ("env-read".into(), 3, "System.getenv".into()),
        ]
    );
}

#[test]
fn unparseable_input_yields_empty() {
    assert_eq!(sites("this is not java ]]]]"), vec![]);
}

// --- process-exec (wave 3) ---

#[test]
fn the_fixed_runtime_chain_and_the_process_builder_constructor_both_emit() {
    let src = "class A {\n  void f(String cmd) throws Exception {\n    Runtime.getRuntime().exec(cmd);\n    new ProcessBuilder(\"sh\", \"-c\", cmd).start();\n  }\n}\n";
    assert_eq!(
        sites(src),
        vec![
            ("process-exec".into(), 3, "Runtime.getRuntime().exec".into()),
            ("process-exec".into(), 4, "new ProcessBuilder".into()),
        ]
    );
}

#[test]
fn an_exec_through_a_runtime_variable_is_silent() {
    // The disclosed cost of structuring this trigger: `rt` is not the fixed platform spelling, so the
    // callee does not resolve and nothing is emitted — recall direction, stated in the consuming
    // rule's own message because the retired bare-word regex used to catch it.
    let src = "class A {\n  void f(String cmd) throws Exception {\n    Runtime rt = Runtime.getRuntime();\n    rt.exec(cmd);\n  }\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn a_user_method_named_exec_is_not_a_site() {
    // THE false-positive class the structural gate retires: the bare word `exec` on any other
    // receiver, or as a declaration, is not Java's process API.
    let src = "class A {\n  void exec(String cmd) {}\n  void f(Runner r, String cmd) {\n    r.exec(cmd);\n    this.exec(cmd);\n  }\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn the_process_builder_start_call_is_not_a_second_site() {
    let src = "class A {\n  void f(String cmd) throws Exception {\n    ProcessBuilder pb = new ProcessBuilder(cmd);\n    pb.start();\n  }\n}\n";
    assert_eq!(
        sites(src),
        vec![("process-exec".into(), 3, "new ProcessBuilder".into())]
    );
}

// --- hash-call (wave 4) ---

fn hash_sites(src: &str) -> Vec<(u32, String, Option<String>)> {
    extract_call_sites("A.java", src)
        .into_iter()
        .filter(|s| s.kind == "hash-call")
        .map(|s| (s.line, s.callee, s.algorithm))
        .collect()
}

#[test]
fn message_digest_get_instance_carries_a_literal_algorithm_verbatim() {
    let src = "class A {
  void f() throws Exception {
    MessageDigest.getInstance(\"MD5\");
    MessageDigest.getInstance(\"SHA-256\");
  }
}
";
    assert_eq!(
        hash_sites(src),
        vec![
            (
                3,
                "MessageDigest.getInstance".to_string(),
                Some("MD5".to_string())
            ),
            (
                4,
                "MessageDigest.getInstance".to_string(),
                Some("SHA-256".to_string())
            ),
        ]
    );
}

#[test]
fn a_dynamic_algorithm_still_emits_a_site_with_none() {
    let src = "class A {
  void f(String algo) throws Exception {
    MessageDigest.getInstance(algo);
  }
}
";
    assert_eq!(
        hash_sites(src),
        vec![(3, "MessageDigest.getInstance".to_string(), None)]
    );
}

#[test]
fn digest_utils_and_cipher_are_silent_but_the_qualified_digest_is_not() {
    // The two exclusions are third-party (`DigestUtils`) and not-a-digest (`Cipher`). The QUALIFIED
    // `java.security.MessageDigest` is recognized on purpose — the JDK tutorial spells it that way and
    // real code follows, so `names_message_digest` accepts a dotted receiver for this family only.
    // Its callee keeps the prefix, because the channel carries the spelling the author wrote.
    let src = "class A {
  void f(byte[] b) throws Exception {
    org.apache.commons.codec.digest.DigestUtils.md5Hex(b);
    Cipher.getInstance(\"DES\");
    java.security.MessageDigest.getInstance(\"MD5\");
  }
}
";
    assert_eq!(
        hash_sites(src),
        vec![(
            5,
            "java.security.MessageDigest.getInstance".to_string(),
            Some("MD5".to_string())
        )]
    );
}
