use super::*;

// One fixture per recognized idiom the module doc names, one per DELIBERATE SILENCE it claims (a
// disclosure nobody tests is a disclosure that drifts), plus the ordering and degrade pins.

/// `(kind, line, callee)` triples — the whole of a site, so a test cannot accidentally assert on a
/// subset and miss a wrong `kind`.
fn sites(src: &str) -> Vec<(String, u32, String)> {
    extract_call_sites("f.py", src)
        .into_iter()
        .map(|s| (s.kind, s.line, s.callee))
        .collect()
}

fn console(line: u32, callee: &str) -> (String, u32, String) {
    (
        CALL_KIND_CONSOLE_WRITE.to_string(),
        line,
        callee.to_string(),
    )
}

fn env(line: u32, callee: &str) -> (String, u32, String) {
    (CALL_KIND_ENV_READ.to_string(), line, callee.to_string())
}

// ---------------------------------------------------------------- console-write

#[test]
fn builtin_print_is_a_console_write() {
    let src = "def f(x):\n    print(x)\n    print()\n";
    assert_eq!(sites(src), vec![console(2, "print"), console(3, "print")]);
}

/// The stream is an ARGUMENT fact this wave does not carry: a stderr `print` is the same site as any
/// other, and the channel makes no stdout claim (module doc).
#[test]
fn print_to_stderr_is_the_same_site_with_no_stream_claim() {
    let src = "import sys\nprint(\"boom\", file=sys.stderr)\n";
    assert_eq!(sites(src), vec![console(2, "print")]);
}

#[test]
fn print_inside_a_string_or_comment_never_fires() {
    let src = "x = \"print(1)\"\n# print(2)\ny = '''\nprint(3)\n'''\n";
    assert!(sites(src).is_empty());
}

// ---------------------------------------------------------------- env-read

#[test]
fn os_getenv_is_an_env_read() {
    let src = "import os\nv = os.getenv(\"HOME\")\n";
    assert_eq!(sites(src), vec![env(2, "os.getenv")]);
}

#[test]
fn os_environ_get_is_an_env_read() {
    let src = "import os\nv = os.environ.get(\"HOME\", \"/\")\n";
    assert_eq!(sites(src), vec![env(2, "os.environ.get")]);
}

/// The subscript form's callee is the MAPPING — `os.environ`, not `os.environ["HOME"]`.
#[test]
fn os_environ_subscript_is_an_env_read_spelled_as_the_mapping() {
    let src = "import os\nv = os.environ[\"HOME\"]\n";
    assert_eq!(sites(src), vec![env(2, "os.environ")]);
}

/// A dynamic key is still a real read point; only the key would be a guess, and the key is not a
/// field (module doc).
#[test]
fn dynamic_env_keys_still_emit() {
    let src =
        "import os\ndef f(k):\n    a = os.environ[k]\n    b = os.getenv(k)\n    return a, b\n";
    assert_eq!(sites(src), vec![env(3, "os.environ"), env(4, "os.getenv")]);
}

/// A WRITE or a DELETE through `os.environ` is not a read — the subscript's `ExprContext` decides.
#[test]
fn os_environ_write_and_delete_are_not_reads() {
    let src =
        "import os\nos.environ[\"A\"] = \"1\"\ndel os.environ[\"B\"]\nos.environ[\"C\"] += \"x\"\n";
    assert!(sites(src).is_empty());
}

/// Bare `os.environ` as a mapping is none of the three idioms (module doc).
#[test]
fn bare_os_environ_mapping_use_is_silent() {
    let src = "import os\nfor k in os.environ:\n    pass\nif \"A\" in os.environ:\n    ks = os.environ.keys()\n";
    assert!(sites(src).is_empty());
}

/// The bare-name form after `from os import getenv` is spelled `getenv`, which is not in the
/// recognized set — a disclosed silence, pinned so widening the set has to be a deliberate edit.
#[test]
fn from_import_bare_getenv_is_silent() {
    let src = "from os import getenv, environ\nv = getenv(\"HOME\")\nw = environ[\"HOME\"]\n";
    assert!(sites(src).is_empty());
}

/// The dotted spelling is reassembled from the attribute chain, so intra-chain whitespace and line
/// continuations normalize away — the one departure from "exactly as written" the module doc names.
#[test]
fn dotted_spelling_normalizes_intra_chain_whitespace() {
    let src = "import os\nv = (os\n     .getenv(\"HOME\"))\n";
    assert_eq!(sites(src), vec![env(2, "os.getenv")]);
}

// ---------------------------------------------------------------- false folds refused

/// Structured loggers are NOT console writes — the channel doc calls folding them in a false fold.
#[test]
fn structured_logger_calls_are_not_console_writes() {
    let src = "import logging\nlogging.info(\"a\")\nlogging.getLogger(__name__).warning(\"b\")\nlog.error(\"c\")\n";
    assert!(sites(src).is_empty());
}

/// A member call named `print` is spelled `self.print` / `logger.print`, which is not `print`.
#[test]
fn member_calls_named_print_are_not_the_builtin() {
    let src = "class A:\n    def go(self):\n        self.print(1)\n        logger.print(2)\n        obj.writer.print(3)\n";
    assert!(sites(src).is_empty());
}

/// A callee that is not a plain name/attribute chain resolves to nothing — never-guess.
#[test]
fn unresolvable_callees_emit_no_site() {
    let src = "def f(mods, i):\n    mods[i].getenv(\"A\")\n    get_os().getenv(\"B\")\n    (lambda: 1)()\n";
    assert!(sites(src).is_empty());
}

// ---------------------------------------------------------------- print rebinding

#[test]
fn a_file_defining_its_own_print_emits_no_console_write() {
    let src = "def print(*a):\n    pass\n\ndef f(x):\n    print(x)\n";
    assert!(sites(src).is_empty());
}

#[test]
fn a_file_importing_print_emits_no_console_write() {
    let src = "from rich import print\nprint(\"hi\")\n";
    assert!(sites(src).is_empty());
}

#[test]
fn a_file_assigning_print_emits_no_console_write() {
    let src = "print = my_writer\nprint(\"hi\")\n";
    assert!(sites(src).is_empty());
}

/// File-scoped with no scope tracking: a parameter (or a class METHOD) named `print` anywhere
/// silences the whole file. Over-silence on purpose — the channel's degrade direction is recall.
#[test]
fn a_parameter_named_print_silences_the_whole_file() {
    let src = "def emit(print):\n    print(\"a\")\n\ndef g():\n    print(\"b\")\n";
    assert!(sites(src).is_empty());
}

#[test]
fn a_class_method_named_print_silences_the_whole_file() {
    let src = "class Report:\n    def print(self):\n        pass\n\nprint(\"b\")\n";
    assert!(sites(src).is_empty());
}

/// Rebinding `print` must not touch the env family — the two are independent claims.
#[test]
fn rebinding_print_leaves_env_reads_alone() {
    let src = "import os\nprint = my_writer\nprint(\"hi\")\nv = os.getenv(\"HOME\")\n";
    assert_eq!(sites(src), vec![env(4, "os.getenv")]);
}

// ---------------------------------------------------------------- f-strings

/// Literal TEXT inside an f-string is text; a real call inside an INTERPOLATION is a real call.
#[test]
fn f_string_literal_text_is_silent_but_an_interpolated_call_emits() {
    let src = "import os\nx = f\"print(1) os.getenv(A)\"\ny = f\"{os.getenv('HOME')}\"\n";
    assert_eq!(sites(src), vec![env(3, "os.getenv")]);
}

// ---------------------------------------------------------------- order and degrade

#[test]
fn sites_come_out_in_source_order_across_kinds_and_lines() {
    let src = "import os\nv = os.environ[\"A\"]\nprint(v)\nw = os.getenv(\"B\")\nprint(os.environ.get(\"C\"))\n";
    assert_eq!(
        sites(src),
        vec![
            env(2, "os.environ"),
            console(3, "print"),
            env(4, "os.getenv"),
            console(5, "print"),
            env(5, "os.environ.get"),
        ]
    );
}

/// A ternary's AST field order (`test` before `body`) is not source order — the sort is what makes
/// the contract true rather than the walk.
#[test]
fn source_order_holds_where_the_ast_field_order_does_not() {
    let src = "import os\nv = print(1) if os.getenv(\"A\") else 2\n";
    assert_eq!(sites(src), vec![console(2, "print"), env(2, "os.getenv")]);
}

#[test]
fn nested_call_emits_outer_before_inner_on_the_same_line() {
    let src = "import os\nprint(os.getenv(\"A\"))\n";
    assert_eq!(sites(src), vec![console(2, "print"), env(2, "os.getenv")]);
}

#[test]
fn parse_failure_yields_empty() {
    assert!(extract_call_sites("bad.py", "def f(:\n").is_empty());
}

#[test]
fn empty_input_yields_empty() {
    assert!(extract_call_sites("empty.py", "").is_empty());
}

#[test]
fn file_with_no_recognized_idiom_yields_empty() {
    assert!(extract_call_sites("f.py", "def f(x):\n    return x + 1\n").is_empty());
}

// --- process-exec (wave 3) ---

#[test]
fn subprocess_and_os_process_apis_emit_with_spelling_as_written() {
    let src = concat!(
        "import subprocess\n",
        "import os\n",
        "\n",
        "def run(cmd):\n",
        "    subprocess.run(cmd)\n",
        "    subprocess.check_output(cmd)\n",
        "    subprocess.Popen(cmd)\n",
        "    os.system(cmd)\n",
        "    os.popen(cmd)\n",
    );
    let got: Vec<(String, u32, String)> = extract_call_sites("a.py", src)
        .into_iter()
        .map(|s| (s.kind, s.line, s.callee))
        .collect();
    assert_eq!(
        got,
        vec![
            ("process-exec".to_string(), 5, "subprocess.run".to_string()),
            (
                "process-exec".to_string(),
                6,
                "subprocess.check_output".to_string()
            ),
            (
                "process-exec".to_string(),
                7,
                "subprocess.Popen".to_string()
            ),
            ("process-exec".to_string(), 8, "os.system".to_string()),
            ("process-exec".to_string(), 9, "os.popen".to_string()),
        ]
    );
}

#[test]
fn a_bare_name_import_of_run_is_silent() {
    // `from subprocess import run` spells `run` at the site — the recognized set is exactly what the
    // consuming rule pins, and widening it is a rule-side change (module doc).
    let src = "from subprocess import run\n\ndef go(cmd):\n    run(cmd)\n";
    assert!(extract_call_sites("a.py", src).is_empty());
}

#[test]
fn third_party_and_in_process_runners_are_silent() {
    let src = concat!(
        "import multiprocessing\n",
        "import sh\n",
        "\n",
        "def go(cmd):\n",
        "    multiprocessing.Process(target=cmd).start()\n",
        "    sh.ls('-l')\n",
    );
    assert!(extract_call_sites("a.py", src).is_empty());
}

// --- hash-call (wave 4) ---

fn hash_sites(src: &str) -> Vec<(u32, String, Option<String>)> {
    extract_call_sites("a.py", src)
        .into_iter()
        .filter(|s| s.kind == "hash-call")
        .map(|s| (s.line, s.callee, s.algorithm))
        .collect()
}

#[test]
fn hashlib_per_algorithm_constructors_name_the_algorithm_in_the_function() {
    let src = "import hashlib

def h(b):
    hashlib.md5(b)
    hashlib.sha256(b)
";
    assert_eq!(
        hash_sites(src),
        vec![
            (4, "hashlib.md5".to_string(), Some("md5".to_string())),
            (5, "hashlib.sha256".to_string(), Some("sha256".to_string())),
        ]
    );
}

#[test]
fn hashlib_new_with_a_literal_carries_it_and_with_a_variable_does_not() {
    // THE never-guess pair: both are real construction sites, only the second leaves the algorithm
    // unspelled — so it fires and an `algorithm_pattern` filter is what loses it.
    let src = "import hashlib

def h(name, b):
    hashlib.new(\"md5\", b)
    hashlib.new(name, b)
";
    assert_eq!(
        hash_sites(src),
        vec![
            (4, "hashlib.new".to_string(), Some("md5".to_string())),
            (5, "hashlib.new".to_string(), None),
        ]
    );
}

#[test]
fn kdfs_hmac_and_bare_name_imports_are_silent() {
    let src = "import hashlib
import hmac
from hashlib import md5

def h(pw, salt, b):
    hashlib.pbkdf2_hmac(\"sha256\", pw, salt, 100000)
    hmac.new(b, b, hashlib.sha256)
    md5(b)
";
    // `hashlib.sha256` passed as an ARGUMENT is not a call and emits nothing either.
    assert_eq!(hash_sites(src), vec![]);
}

#[test]
fn an_md5_named_only_in_a_string_or_comment_is_not_a_site() {
    let src = "import hashlib

def doc():
    # hashlib.md5(b)
    return \"hashlib.md5(b)\"
";
    assert_eq!(hash_sites(src), vec![]);
}
