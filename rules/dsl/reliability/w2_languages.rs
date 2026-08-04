use crate::{env_config_overlay, hits, scan, scan_with, TempDir};

// --- call-scan wave 2: Go, Java, C# (console-write + env-read) and Rust (env-read only) ---
//
// W1 landed the channel with TypeScript and Python producers; these tests are the CONSUMING half of
// wave 2, written red-first: each positive was observed silent before the producer arms were wired
// into `pipeline/fresh/call_sites.rs` and the three rules' `file_pattern`s admitted the extensions.
// The negatives pin each producer's disclosed boundary from the rule side, so a later widening of a
// producer's family cannot land without a rule-level behavior change turning something red here.
//
// Rust is deliberately present ONLY under `env-outside-config`: the design doc rules `println!` a
// permanent blank for the console rules (a CLI's normal output), so no `.rs` console test exists in
// any direction other than the pin that the console rules stay silent on `.rs` — see
// `a_rust_println_never_reaches_the_console_rules` at the bottom.

fn fires(rule: &str, path: &str, src: &str, line: u32) {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(path, src);
    let out = scan(&dir);
    let h = hits(&out, rule);
    assert_eq!(h.len(), 1, "{rule} on {path}: {:?}", out.findings);
    assert_eq!(h[0].line, line, "{rule} on {path}: {:?}", out.findings);
}

fn silent(rule: &str, path: &str, src: &str) {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(path, src);
    let out = scan(&dir);
    assert!(
        hits(&out, rule).is_empty(),
        "{rule} on {path}: {:?}",
        out.findings
    );
}

// --- console-in-be: the backend-path gate is language-neutral, the callee set is per-language ---

#[test]
fn a_go_fmt_println_on_a_backend_path_fires() {
    fires(
        "console-in-be",
        "api/handler.go",
        "package api\n\nimport \"fmt\"\n\nfunc Handle() {\n\tfmt.Println(\"request\")\n}\n",
        6,
    );
}

#[test]
fn a_go_fprintf_to_stderr_on_a_backend_path_fires() {
    // The `Fprint*` trio joins the family only when the writer is SPELLED `os.Stdout`/`os.Stderr` at
    // the site — the producer's family test, consumed here through the rule's callee_pattern.
    fires(
        "console-in-be",
        "services/report.go",
        "package services\n\nimport (\n\t\"fmt\"\n\t\"os\"\n)\n\nfunc Report() {\n\tfmt.Fprintf(os.Stderr, \"boom\\n\")\n}\n",
        9,
    );
}

#[test]
fn a_go_log_println_is_not_a_console_write() {
    // The design doc's named boundary case: stdlib `log` defaults to stderr but is a CONFIGURABLE
    // logger (`log.SetOutput`), so v1 excludes it — producer module doc owns the disclosure.
    silent(
        "console-in-be",
        "api/handler.go",
        "package api\n\nimport \"log\"\n\nfunc Handle() {\n\tlog.Println(\"request\")\n}\n",
    );
}

#[test]
fn a_java_system_out_println_on_a_backend_path_fires() {
    fires(
        "console-in-be",
        "services/Handler.java",
        "class Handler {\n  void handle() {\n    System.out.println(\"request\");\n  }\n}\n",
        3,
    );
}

#[test]
fn a_java_slf4j_logger_is_not_a_console_write() {
    silent(
        "console-in-be",
        "services/Handler.java",
        "class Handler {\n  private static final Logger log = LoggerFactory.getLogger(Handler.class);\n  void handle() {\n    log.info(\"request\");\n  }\n}\n",
    );
}

#[test]
fn a_csharp_console_writeline_on_a_backend_path_fires() {
    fires(
        "console-in-be",
        "controllers/Handler.cs",
        "class Handler {\n  void Handle() {\n    Console.WriteLine(\"request\");\n  }\n}\n",
        3,
    );
}

#[test]
fn a_csharp_ilogger_is_not_a_console_write() {
    silent(
        "console-in-be",
        "controllers/Handler.cs",
        "class Handler {\n  private readonly ILogger<Handler> _logger;\n  void Handle() {\n    _logger.LogInformation(\"request\");\n  }\n}\n",
    );
}

#[test]
fn the_backend_path_gate_stays_language_neutral() {
    // Same console write, no backend-ish segment in the path — the WHERE half of the rule is the same
    // path judgment for every language, no per-language carve-out.
    silent(
        "console-in-be",
        "cmd/tool.go",
        "package main\n\nimport \"fmt\"\n\nfunc main() {\n\tfmt.Println(\"cli output\")\n}\n",
    );
}

#[test]
fn the_ok_marker_suppresses_in_the_new_languages_too() {
    silent(
        "console-in-be",
        "api/handler.go",
        "package api\n\nimport \"fmt\"\n\nfunc Handle() {\n\t// zzop-console-in-be-ok: startup banner, vetted\n\tfmt.Println(\"request\")\n}\n",
    );
    silent(
        "console-in-be",
        "services/Handler.java",
        "class Handler {\n  void handle() {\n    // zzop-console-in-be-ok: startup banner, vetted\n    System.out.println(\"request\");\n  }\n}\n",
    );
}

// --- console-in-loop: in_loop crosses the new producers with each language's own loop_spans ---

#[test]
fn a_go_fmt_println_inside_a_for_statement_fires() {
    fires(
        "console-in-loop",
        "src/report.go",
        "package main\n\nimport \"fmt\"\n\nfunc report(rows []string) {\n\tfor _, row := range rows {\n\t\tfmt.Println(row)\n\t}\n}\n",
        7,
    );
}

#[test]
fn the_same_go_write_outside_the_loop_is_silent() {
    silent(
        "console-in-loop",
        "src/report.go",
        "package main\n\nimport \"fmt\"\n\nfunc report(rows []string) {\n\tfor _, row := range rows {\n\t\taccumulate(row)\n\t}\n\tfmt.Println(len(rows))\n}\n",
    );
}

#[test]
fn a_java_write_inside_an_enhanced_for_fires() {
    fires(
        "console-in-loop",
        "src/Report.java",
        "class Report {\n  void report(java.util.List<String> rows) {\n    for (String row : rows) {\n      System.out.println(row);\n    }\n  }\n}\n",
        4,
    );
}

#[test]
fn a_java_write_inside_a_stream_lambda_is_silent() {
    // Java's `loop_spans` deliberately exclude Stream-pipeline lambdas (lazy — the producer's
    // eager/lazy boundary), so containment is never proven here. Inherited, not re-decided.
    silent(
        "console-in-loop",
        "src/Report.java",
        "class Report {\n  void report(java.util.stream.Stream<String> rows) {\n    rows.map(row -> {\n      System.out.println(row);\n      return row;\n    });\n  }\n}\n",
    );
}

#[test]
fn a_csharp_write_inside_a_foreach_fires() {
    fires(
        "console-in-loop",
        "src/Report.cs",
        "class Report {\n  void Run(string[] rows) {\n    foreach (var row in rows) {\n      Console.WriteLine(row);\n    }\n  }\n}\n",
        4,
    );
}

#[test]
fn the_same_csharp_write_outside_the_loop_is_silent() {
    silent(
        "console-in-loop",
        "src/Report.cs",
        "class Report {\n  void Run(string[] rows) {\n    foreach (var row in rows) {\n      Accumulate(row);\n    }\n    Console.WriteLine(rows.Length);\n  }\n}\n",
    );
}

// --- env-outside-config: four more languages under one declaration gate ---

fn env_fires(path: &str, src: &str, lines: &[u32]) {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(path, src);
    let out = scan_with(&dir, env_config_overlay(&["src/config"]));
    let h = hits(&out, "env-outside-config");
    let got: Vec<u32> = h.iter().map(|f| f.line).collect();
    assert_eq!(got, lines, "{path}: {:?}", out.findings);
}

fn env_silent(path: &str, src: &str) {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(path, src);
    let out = scan_with(&dir, env_config_overlay(&["src/config"]));
    assert!(
        hits(&out, "env-outside-config").is_empty(),
        "{path}: {:?}",
        out.findings
    );
}

#[test]
fn a_go_env_read_outside_the_declared_module_fires() {
    env_fires(
        "src/db.go",
        "package db\n\nimport \"os\"\n\nfunc dsn() string {\n\treturn os.Getenv(\"DATABASE_URL\")\n}\n\nfunc port() (string, bool) {\n\treturn os.LookupEnv(\"PORT\")\n}\n",
        &[6, 10],
    );
}

#[test]
fn a_java_system_getenv_outside_the_declared_module_fires() {
    env_fires(
        "src/Db.java",
        "class Db {\n  String dsn() {\n    return System.getenv(\"DATABASE_URL\");\n  }\n}\n",
        &[3],
    );
}

#[test]
fn a_csharp_environment_read_outside_the_declared_module_fires() {
    env_fires(
        "src/Db.cs",
        "class Db {\n  string Dsn() {\n    return Environment.GetEnvironmentVariable(\"DATABASE_URL\");\n  }\n}\n",
        &[3],
    );
}

#[test]
fn a_rust_std_env_var_outside_the_declared_module_fires() {
    // Both spellings of the same function are each their own callee — the channel's
    // original-spelling contract, visible from the rule side as two findings with two spellings.
    env_fires(
        "src/db.rs",
        "use std::env;\n\nfn dsn() -> String {\n    std::env::var(\"DATABASE_URL\").unwrap_or_else(|_| env::var(\"DB_URL\").unwrap())\n}\n",
        &[4, 4],
    );
}

#[test]
fn a_rust_compile_time_env_macro_is_not_an_env_read() {
    // `env!()` resolves at COMPILE time and reads no process environment at run time — the channel
    // constant's own named boundary, pinned from the rule side.
    env_silent(
        "src/version.rs",
        "pub fn version() -> &'static str {\n    env!(\"CARGO_PKG_VERSION\")\n}\n",
    );
}

#[test]
fn a_read_inside_the_declared_config_module_is_exempt_in_every_language() {
    env_silent(
        "src/config/env.go",
        "package config\n\nimport \"os\"\n\nfunc Load() string {\n\treturn os.Getenv(\"DATABASE_URL\")\n}\n",
    );
    env_silent(
        "src/config/env.rs",
        "pub fn load() -> String {\n    std::env::var(\"DATABASE_URL\").unwrap()\n}\n",
    );
}

#[test]
fn the_env_ok_marker_suppresses_in_the_new_languages_too() {
    env_silent(
        "src/db.rs",
        "fn dsn() -> String {\n    // zzop-env-outside-config-ok: bootstrap probe, vetted\n    std::env::var(\"DATABASE_URL\").unwrap()\n}\n",
    );
}

// --- the Rust console permanent blank, pinned from the rule side ---

#[test]
fn a_rust_println_never_reaches_the_console_rules() {
    // Two silences stacked on purpose, and the test holds while EITHER stands: the producer emits no
    // `console-write` for `.rs` (the `println!` judgment — a CLI's normal output), and the two console
    // rules' `file_pattern`s never admit `.rs`. If someone reverses one half without the other, this
    // still passes — the pin is on the OBSERVABLE promise (no `.rs` console finding), which is the
    // part the design doc marks permanent.
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "services/report.rs",
        "fn report(rows: &[String]) {\n    for row in rows {\n        println!(\"{row}\");\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "console-in-be").is_empty() && hits(&out, "console-in-loop").is_empty(),
        "{:?}",
        out.findings
    );
}
