use super::*;

fn sites(src: &str) -> Vec<(String, u32, String)> {
    extract_call_sites("A.cs", src)
        .into_iter()
        .map(|s| (s.kind, s.line, s.callee))
        .collect()
}

// --- console-write ---

#[test]
fn console_write_and_writeline_emit_with_and_without_system_prefix() {
    let src = "class A {\n  void F() {\n    Console.WriteLine(\"a\");\n    Console.Write(\"b\");\n    System.Console.WriteLine(\"c\");\n  }\n}\n";
    assert_eq!(
        sites(src),
        vec![
            ("console-write".into(), 3, "Console.WriteLine".into()),
            ("console-write".into(), 4, "Console.Write".into()),
            ("console-write".into(), 5, "System.Console.WriteLine".into()),
        ]
    );
}

#[test]
fn console_error_and_out_writer_properties_emit_with_the_property_in_the_callee() {
    let src = "class A {\n  void F() {\n    Console.Error.WriteLine(\"a\");\n    System.Console.Error.Write(\"b\");\n    Console.Out.WriteLine(\"c\");\n  }\n}\n";
    assert_eq!(
        sites(src),
        vec![
            ("console-write".into(), 3, "Console.Error.WriteLine".into()),
            (
                "console-write".into(),
                4,
                "System.Console.Error.Write".into()
            ),
            ("console-write".into(), 5, "Console.Out.WriteLine".into()),
        ]
    );
}

#[test]
fn ilogger_and_serilog_are_deliberately_silent() {
    // The FALSE-FOLD boundary the channel doc owns: configured output with levels and sinks is not a
    // console write, and a rule banning console writes is not banning logging.
    let src = "class A {\n  private readonly ILogger<A> _logger;\n  void F() {\n    _logger.LogInformation(\"x\");\n    _logger.LogError(e, \"y\");\n    Log.Information(\"z\");\n  }\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn non_writing_console_members_are_silent() {
    let src =
        "class A {\n  void F() {\n    var s = Console.ReadLine();\n    Console.Clear();\n  }\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn using_static_bare_writeline_is_silent() {
    // Its spelling at the site is `WriteLine`, not a chain naming `Console` (module doc).
    let src =
        "using static System.Console;\nclass A {\n  void F() {\n    WriteLine(\"x\");\n  }\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn aliased_writer_is_silent() {
    let src =
        "class A {\n  void F() {\n    var w = Console.Error;\n    w.WriteLine(\"x\");\n  }\n}\n";
    assert_eq!(sites(src), vec![]);
}

// --- env-read ---

#[test]
fn get_environment_variable_emits_with_and_without_system_prefix() {
    let src = "class A {\n  void F(string name) {\n    var a = Environment.GetEnvironmentVariable(\"PORT\");\n    var b = System.Environment.GetEnvironmentVariable(name);\n  }\n}\n";
    assert_eq!(
        sites(src),
        vec![
            (
                "env-read".into(),
                3,
                "Environment.GetEnvironmentVariable".into()
            ),
            (
                "env-read".into(),
                4,
                "System.Environment.GetEnvironmentVariable".into()
            ),
        ]
    );
}

#[test]
fn bulk_and_write_environment_calls_are_silent() {
    let src = "class A {\n  void F() {\n    var all = Environment.GetEnvironmentVariables();\n    Environment.SetEnvironmentVariable(\"A\", \"1\");\n  }\n}\n";
    assert_eq!(sites(src), vec![]);
}

// --- text boundaries and degrade ---

#[test]
fn string_and_comment_mentions_never_fire() {
    let src = "class A {\n  // Console.WriteLine(\"comment\")\n  string F() {\n    return \"Console.WriteLine(Environment.GetEnvironmentVariable(x))\";\n  }\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn call_nested_in_an_argument_emits_both_outer_first() {
    let src = "class A {\n  void F() {\n    Console.WriteLine(Environment.GetEnvironmentVariable(\"HOME\"));\n  }\n}\n";
    assert_eq!(
        sites(src),
        vec![
            ("console-write".into(), 3, "Console.WriteLine".into()),
            (
                "env-read".into(),
                3,
                "Environment.GetEnvironmentVariable".into()
            ),
        ]
    );
}

#[test]
fn unparseable_input_yields_empty() {
    assert_eq!(sites("definitely not c sharp }}}}"), vec![]);
}

// --- process-exec (wave 3) ---

#[test]
fn process_start_and_the_start_info_constructor_both_emit() {
    let src = "class A {\n  void F(string cmd) {\n    Process.Start(\"sh\", cmd);\n    System.Diagnostics.Process.Start(cmd);\n    var psi = new ProcessStartInfo(\"sh\", cmd);\n  }\n}\n";
    assert_eq!(
        sites(src),
        vec![
            ("process-exec".into(), 3, "Process.Start".into()),
            (
                "process-exec".into(),
                4,
                "System.Diagnostics.Process.Start".into()
            ),
            ("process-exec".into(), 5, "new ProcessStartInfo".into()),
        ]
    );
}

#[test]
fn an_instance_start_is_silent() {
    // The receiver is a variable this producer does not resolve (module doc) — and a real launch
    // usually carries its `ProcessStartInfo` construction as the witness anyway.
    let src = "class A {\n  void F(System.Diagnostics.Process proc) {\n    proc.Start();\n  }\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn non_launching_process_members_are_silent() {
    let src = "class A {\n  void F(int id) {\n    var p = Process.GetProcessById(id);\n  }\n}\n";
    assert_eq!(sites(src), vec![]);
}

// --- hash-call (wave 4) ---

fn hash_sites(src: &str) -> Vec<(u32, String, Option<String>)> {
    extract_call_sites("A.cs", src)
        .into_iter()
        .filter(|s| s.kind == "hash-call")
        .map(|s| (s.line, s.callee, s.algorithm))
        .collect()
}

#[test]
fn per_algorithm_factories_name_the_algorithm_in_the_type() {
    let src = "class A {
  void F() {
    MD5.Create();
    System.Security.Cryptography.SHA1.Create();
    SHA256.Create();
  }
}
";
    assert_eq!(
        hash_sites(src),
        vec![
            (3, "MD5.Create".to_string(), Some("MD5".to_string())),
            (
                4,
                "System.Security.Cryptography.SHA1.Create".to_string(),
                Some("SHA1".to_string())
            ),
            (5, "SHA256.Create".to_string(), Some("SHA256".to_string())),
        ]
    );
}

#[test]
fn the_generic_factory_carries_a_literal_and_refuses_a_variable() {
    let src = "class A {
  void F(string algo) {
    HashAlgorithm.Create(\"MD5\");
    HashAlgorithm.Create(algo);
  }
}
";
    assert_eq!(
        hash_sites(src),
        vec![
            (
                3,
                "HashAlgorithm.Create".to_string(),
                Some("MD5".to_string())
            ),
            (4, "HashAlgorithm.Create".to_string(), None),
        ]
    );
}

#[test]
fn hmac_ciphers_and_third_party_hashes_are_silent() {
    let src = "class A {
  void F(byte[] b) {
    new HMACSHA1(b);
    Aes.Create();
    BCrypt.Net.BCrypt.HashPassword(\"x\");
  }
}
";
    assert_eq!(hash_sites(src), vec![]);
}
