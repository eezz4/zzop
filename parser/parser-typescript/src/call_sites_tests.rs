//! `extract_call_sites` coverage — the recognized idiom per kind, and one negative per deliberate
//! silence the producer's module doc claims (string/comment, structured logger, dynamic member,
//! non-bare receiver, bare `process.env`, `import.meta.env`). Split out of `call_sites.rs` for the
//! usual reason (`symbols_tests.rs`'s): source + these tests would exceed the 300-line cap.
//!
//! Every assertion compares the WHOLE site list, not a `contains` — an over-emitting producer is as
//! wrong as a silent one here, and a negative test that only checks "the expected site is absent"
//! would pass on a producer that emitted something else entirely.

use crate::{extract_call_sites, CONSOLE_WRITE_METHODS};
use zzop_core::{CALL_KIND_CONSOLE_WRITE, CALL_KIND_ENV_READ};

fn sites(src: &str) -> Vec<(String, u32, String)> {
    extract_call_sites("f.ts", src)
        .into_iter()
        .map(|s| (s.kind, s.line, s.callee))
        .collect()
}

fn console(line: u32, method: &str) -> (String, u32, String) {
    (
        CALL_KIND_CONSOLE_WRITE.to_string(),
        line,
        format!("console.{method}"),
    )
}

fn env(line: u32) -> (String, u32, String) {
    (
        CALL_KIND_ENV_READ.to_string(),
        line,
        "process.env".to_string(),
    )
}

// --- console-write ---

/// Every method in the vocabulary is recognized, and `callee` keeps the receiver — the spelling IS
/// what the consuming rule's `callee_pattern` matches on.
#[test]
fn every_console_method_emits_with_its_receiver_qualified_spelling() {
    let src = CONSOLE_WRITE_METHODS
        .iter()
        .map(|m| format!("console.{m}(\"x\");\n"))
        .collect::<String>();
    let expected: Vec<_> = CONSOLE_WRITE_METHODS
        .iter()
        .enumerate()
        .map(|(i, m)| console(i as u32 + 1, m))
        .collect();
    assert_eq!(sites(&src), expected);
}

#[test]
fn console_write_line_is_the_receiver_line_of_a_multiline_call() {
    let src = "console\n  .error(\n    \"boom\",\n  );\n";
    assert_eq!(sites(src), vec![console(1, "error")]);
}

#[test]
fn optional_console_call_forms_emit() {
    let src = "console?.log(1);\nconsole.warn?.(2);\n";
    assert_eq!(sites(src), vec![console(1, "log"), console(2, "warn")]);
}

/// A string literal and a comment both spell `console.log` — the two failure modes a line regex has
/// and this channel exists to remove.
#[test]
fn console_log_inside_a_string_or_a_comment_is_not_a_site() {
    let src = "const s = \"console.log(x)\";\n// console.log(x)\n/* console.error(x) */\nconst t = `console.info(x)`;\n";
    assert!(sites(src).is_empty());
}

/// A structured logger is NOT a console write — folding it in would be false (see the module doc).
#[test]
fn structured_logger_calls_are_not_console_writes() {
    let src = "logger.info(\"a\");\nwinston.error(\"b\");\nthis.logger.warn(\"c\");\npino().debug(\"d\");\n";
    assert!(sites(src).is_empty());
}

#[test]
fn dynamic_or_non_bare_console_receivers_emit_nothing() {
    let src = "console[m](\"x\");\nglobalThis.console.log(\"y\");\nconst c = console;\nc.log(\"z\");\nconsole.table([]);\n";
    assert!(sites(src).is_empty());
}

/// Referencing the function without calling it is not a write.
#[test]
fn console_method_reference_without_a_call_is_not_a_site() {
    let src = "const f = console.log;\nsubscribe(console.error);\n";
    assert!(sites(src).is_empty());
}

// --- env-read ---

#[test]
fn member_and_bracket_env_reads_both_emit_the_receiver_as_callee() {
    let src = "const a = process.env.API_URL;\nconst b = process.env[\"PORT\"];\nconst c = process.env?.HOST;\n";
    assert_eq!(sites(src), vec![env(1), env(2), env(3)]);
}

/// Chained access past the variable must emit exactly one site, not one per member in the chain.
#[test]
fn chained_access_off_an_env_variable_emits_one_site() {
    let src = "const a = process.env.NODE_ENV.trim();\n";
    assert_eq!(sites(src), vec![env(1)]);
}

/// A computed key still names a key POSITION, and the key is not carried — so this is a real env read
/// and emitting it guesses nothing. Pinned identically in the Python producer (`os.environ[k]`): the
/// two producers must agree, or one language's `env-read` population differs for no stateable reason.
#[test]
fn a_computed_key_still_emits_an_env_read() {
    let src = "const a = process.env[key];\nconst b = process.env[`P${x}`];\n";
    assert_eq!(sites(src), vec![env(1), env(2)]);
}

/// The other half of the old pairing: with NO key position at all there is nothing to read AT — a
/// capture of the whole object is not a keyed read, in either producer.
#[test]
fn bare_receiver_and_destructuring_emit_nothing() {
    let src = "const e = process.env;\nconst { PORT } = process.env;\nObject.keys(process.env);\n";
    assert!(sites(src).is_empty());
}

/// Build-time metadata, not a process environment read — the boundary `CALL_KIND_ENV_READ` draws.
#[test]
fn import_meta_env_is_not_an_env_read() {
    let src = "const a = import.meta.env.VITE_API;\nconst b = import.meta.env[\"VITE_X\"];\n";
    assert!(sites(src).is_empty());
}

// --- ordering and robustness ---

#[test]
fn interleaved_kinds_are_emitted_in_source_order() {
    let src = "console.log(process.env.A);\nconst b = process.env.B;\nconsole.error(\"x\");\nfn(process.env.C, console.warn(\"y\"));\n";
    assert_eq!(
        sites(src),
        vec![
            console(1, "log"),
            env(1),
            env(2),
            console(3, "error"),
            env(4),
            console(4, "warn"),
        ]
    );
}

#[test]
fn empty_and_unparseable_input_yield_no_sites_without_panicking() {
    for src in [
        "",
        "\n\n",
        "function f( {\n console.log(1)\n",
        "}\nprocess.env.A;\n",
    ] {
        assert!(extract_call_sites("f.ts", src).is_empty(), "{src:?}");
    }
}

#[test]
fn a_decorated_class_prop_emits_sites_in_source_order_decorator_call_first() {
    // RED, reproduced pre-repair (2026-08-03 review, same class as the string_literals sibling's
    // decorator pin): swc walks a `ClassProp`'s VALUE before its `decorators` field (AST struct
    // order, not source order), so a console write inside a decorator ARGUMENT emitted after the
    // property value's own site — [(line 3), (line 2)] — violating this module's "source order"
    // claim, which the walk alone therefore does not deliver. The producer now sorts by source
    // offset before returning; the module doc records why.
    let src = concat!(
        "class C {\n",
        "  @Dec(console.log(1))\n",
        "  x = console.log(2);\n",
        "}\n",
    );
    assert_eq!(sites(src), vec![console(2, "log"), console(3, "log")]);
}

// --- process-exec (wave 3): binding resolution, not spelling ---

fn exec(line: u32, callee: &str) -> (String, u32, String) {
    (
        zzop_core::CALL_KIND_PROCESS_EXEC.to_string(),
        line,
        callee.to_string(),
    )
}

#[test]
fn a_named_import_of_exec_makes_a_bare_call_a_site() {
    let src = "import { exec } from \"child_process\";\n\nexport function run(cmd: string) {\n  exec(cmd);\n}\n";
    assert_eq!(sites(src), vec![exec(4, "exec")]);
}

#[test]
fn an_aliased_named_import_keeps_the_alias_as_the_callee() {
    // The original-spelling contract: the channel carries what the author wrote, so a rule that
    // wants the alias can see it and one that wants the family reads `kind`.
    let src = "import { execSync as sh } from \"node:child_process\";\n\nexport function run(c: string) {\n  sh(c);\n}\n";
    assert_eq!(sites(src), vec![exec(4, "sh")]);
}

#[test]
fn a_namespace_import_makes_member_calls_sites() {
    let src = "import * as cp from \"child_process\";\nimport childProcess from \"child_process\";\n\nexport function run(c: string) {\n  cp.exec(c);\n  childProcess.spawnSync(c);\n}\n";
    assert_eq!(
        sites(src),
        vec![exec(5, "cp.exec"), exec(6, "childProcess.spawnSync")]
    );
}

#[test]
fn both_require_binding_shapes_resolve() {
    let src = "const cp = require(\"child_process\");\nconst { execFile } = require(\"child_process\");\n\nfunction run(c) {\n  cp.execSync(c);\n  execFile(c);\n}\n";
    assert_eq!(
        sites(src),
        vec![exec(5, "cp.execSync"), exec(6, "execFile")]
    );
}

#[test]
fn a_regexp_exec_is_not_a_site_even_in_a_child_process_file() {
    // THE false-positive class this producer exists to retire: the bare word `exec` is `RegExp`'s
    // method name too, and the consuming rule's old `require_file: "child_process"` pre-gate could
    // not tell them apart in a file that legitimately uses both.
    let src = "import { spawn } from \"child_process\";\n\nexport function parse(re: RegExp, s: string) {\n  return re.exec(s);\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn an_unimported_bare_exec_is_not_a_site() {
    let src = "function exec(cmd: string) {}\n\nexport function run(c: string) {\n  exec(c);\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn a_third_party_runner_is_not_this_family() {
    // `execa` is not the platform's API — the consuming rules' shell/argv claims are stated about
    // Node's `child_process`, so folding a wrapper in would attach them to unverified semantics.
    let src = "import { execa } from \"execa\";\nimport { $ } from \"zx\";\n\nexport async function run(c: string) {\n  await execa(c);\n  await $`${c}`;\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn a_fork_call_is_not_this_family() {
    let src = "import { fork } from \"child_process\";\n\nexport function run(m: string) {\n  fork(m);\n}\n";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn an_exec_named_only_in_a_string_or_comment_is_not_a_site() {
    let src = "import { exec } from \"child_process\";\n\nexport function doc(): string {\n  // exec(cmd)\n  return \"exec(cmd)\";\n}\n";
    assert_eq!(sites(src), vec![]);
}

// --- hash-call (wave 4): the channel's one argument-derived fact, and its never-guess line ---

fn hash(line: u32, callee: &str, algorithm: Option<&str>) -> (String, u32, String) {
    // The tuple helper above carries no algorithm, so these tests read the sites directly instead.
    let _ = algorithm;
    (
        zzop_core::CALL_KIND_HASH_CALL.to_string(),
        line,
        callee.to_string(),
    )
}

fn algorithms(src: &str) -> Vec<Option<String>> {
    extract_call_sites("f.ts", src)
        .into_iter()
        .filter(|s| s.kind == zzop_core::CALL_KIND_HASH_CALL)
        .map(|s| s.algorithm)
        .collect()
}

#[test]
fn a_named_import_of_create_hash_with_a_string_literal_carries_the_algorithm() {
    let src = "import { createHash } from \"crypto\";

export function h(b: Uint8Array) {
  return createHash(\"md5\").update(b).digest();
}
";
    assert_eq!(sites(src), vec![hash(4, "createHash", Some("md5"))]);
    assert_eq!(algorithms(src), vec![Some("md5".to_string())]);
}

#[test]
fn the_algorithm_spelling_is_the_authors_not_normalized() {
    // Case is the author's — a consuming rule owns its own case-insensitivity (the original-spelling
    // contract every field on this channel is under).
    let src = "import crypto from \"node:crypto\";

export function h(b: Uint8Array) {
  return crypto.createHash(\"MD5\").update(b).digest();
}
";
    assert_eq!(sites(src), vec![hash(4, "crypto.createHash", Some("MD5"))]);
    assert_eq!(algorithms(src), vec![Some("MD5".to_string())]);
}

#[test]
fn a_dynamic_algorithm_still_emits_a_site_but_carries_no_algorithm() {
    // THE never-guess pin: the digest construction is real and witnessed, the algorithm is not spelled
    // at the site, so the site fires and only an `algorithm_pattern` filter loses it.
    let src = "import { createHash } from \"crypto\";

export function h(algo: string, b: Uint8Array) {
  return createHash(algo).update(b).digest();
}
";
    assert_eq!(sites(src), vec![hash(4, "createHash", None)]);
    assert_eq!(algorithms(src), vec![None]);
}

#[test]
fn a_no_substitution_template_literal_is_not_a_spelled_algorithm() {
    // Deliberate: reading a template's cooked value is the first step onto the argument-capture slope
    // the channel refuses. The site fires, the algorithm does not.
    let src = "import { createHash } from \"crypto\";

export function h(b: Uint8Array) {
  return createHash(`md5`).update(b).digest();
}
";
    assert_eq!(algorithms(src), vec![None]);
}

#[test]
fn an_unimported_create_hash_is_not_a_site() {
    // The same resolver `process-exec` uses: a local helper named `createHash` is not Node's crypto.
    let src = "function createHash(algo: string) {
  return { update: (b: Uint8Array) => ({ digest: () => \"\" }) };
}

export function h(b: Uint8Array) {
  return createHash(\"md5\").update(b).digest();
}
";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn a_third_party_digest_package_is_not_this_family() {
    let src = "import md5 from \"js-md5\";
import CryptoJS from \"crypto-js\";

export function h(s: string) {
  return md5(s) + CryptoJS.MD5(s);
}
";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn a_cipher_construction_is_not_a_digest() {
    let src = "import { createCipheriv } from \"crypto\";

export function e(k: Buffer, iv: Buffer) {
  return createCipheriv(\"aes-256-gcm\", k, iv);
}
";
    assert_eq!(sites(src), vec![]);
}

#[test]
fn an_md5_named_only_in_a_string_or_comment_is_not_a_site() {
    let src = "import { createHash } from \"crypto\";

export function doc(): string {
  // createHash(\"md5\")
  return \"createHash(md5)\";
}
";
    assert_eq!(sites(src), vec![]);
}
