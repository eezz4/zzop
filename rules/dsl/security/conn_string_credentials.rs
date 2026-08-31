use crate::{hits, scan, TempDir};

// --- conn-string-credentials ---

#[test]
fn postgres_connection_string_with_password_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "export const url = \"postgres://user:hunter2@host:5432/db\";\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "conn-string-credentials");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn redis_connection_string_with_password_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/cache.ts",
        "export const url = \"redis://user:pass123@host:6379\";\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "conn-string-credentials").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn mongodb_srv_connection_string_with_password_is_flagged() {
    // Also exercises the general RFC 3986 scheme grammar (`[a-z][a-z0-9+.-]*`) against a scheme
    // carrying a `+` (`mongodb+srv`), not just a plain alphabetic scheme.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/mongo.ts",
        "export const url = \"mongodb+srv://admin:realsecret@cluster.mongodb.net/db\";\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "conn-string-credentials").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn connection_string_with_env_var_interpolation_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "export const url = `postgres://user:${process.env.DB_PASSWORD}@host:5432/db`;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn connection_string_with_angle_bracket_placeholder_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "export const url = \"postgres://user:<password>@host:5432/db\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn connection_string_with_mustache_placeholder_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "export const url = \"postgres://user:{{password}}@host:5432/db\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn real_credential_with_a_ts_generic_elsewhere_on_the_line_is_still_flagged() {
    // The placeholder vetoes are anchored to the URL's userinfo (between `://` and `@`) — a TS
    // generic's angle brackets outside the URL must not suppress a real literal credential.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "export const c: Record<string, string> = { db: \"postgres://svc:S3cr3tPw9@host:5432/db\" };\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "conn-string-credentials").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn real_credential_with_interpolated_host_after_the_at_sign_is_still_flagged() {
    // `${...}` interpolation in the HOST slot does not launder a literal password in the
    // userinfo slot — only a placeholder between `://` and `@` vetoes.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "export const url = `postgres://admin:S3cr3tPw9@${host}/db`;\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "conn-string-credentials").len(),
        1,
        "{:?}",
        out.findings
    );
}

// --- the Rust `format!` placeholder lane (the `.rs` false-positive class) ---
//
// The arc that added `.rs` to this rule's `file_pattern` left the userinfo placeholder veto JS-only
// (`${...}` / `{{...}}` / `<...>` / `process.env`), so Rust's `{}` / `{name}` format placeholders sailed
// through and a `format!("postgresql://{user}:{password}@{host}...")` fired at CRITICAL with a message
// ordering the reader to ROTATE a credential that is not in the source at all. The veto vocabulary is
// what was language-blind, so that is what was widened — still anchored between `://` and `@`, so a
// placeholder in the HOST slot keeps laundering nothing.

#[test]
fn rust_format_named_placeholder_connection_string_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/db.rs",
        "pub fn url(user: &str, password: &str, host: &str) -> String {\n    format!(\"postgresql://{user}:{password}@{host}:5432/app\")\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn rust_format_positional_placeholder_connection_string_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/db.rs",
        "pub fn url(u: &str, p: &str, h: &str) -> String {\n    format!(\"postgresql://{}:{}@{}:5432/app\", u, p, h)\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The under-detection boundary the widened veto must NOT cross: a real password spelled literally in
/// Rust source is still a leaked credential, and this is the line the rule exists for. Dropping `.rs`
/// from the `file_pattern` would have silenced it.
#[test]
fn a_literal_credential_in_rust_source_is_still_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/db.rs",
        "pub const DB_URL: &str = \"postgresql://svc:hunter2@db.prod:5432/app\";\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "conn-string-credentials");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

/// Second half of that boundary, and the reason the new arm was added to the userinfo-anchored
/// alternative rather than to the line at large: a Rust placeholder in the HOST slot does not launder a
/// literal password sitting in the userinfo slot — exactly the standing the `${...}` sibling already had.
#[test]
fn a_rust_format_with_a_literal_password_and_an_interpolated_host_is_still_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/db.rs",
        "pub fn url(host: &str) -> String {\n    format!(\"postgresql://svc:hunter2@{host}:5432/app\")\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "conn-string-credentials");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

// --- the printf-VERB placeholder lane (the Go/Python/C `%s` false-positive class) ---
//
// The userinfo placeholder veto knew four spellings of "the password is not on this line" — `${...}`,
// `{{...}}`/`<...>`, `process.env`, and Rust's brace format placeholders — and none of them is how the
// printf family writes one. So `fmt.Sprintf("postgresql://%s:%s@%s:%s/%s", ...)` fired at CRITICAL with
// a message ordering the reader to ROTATE a credential the line does not contain.
//
// The measured distribution is deliberately NOT restated here. The rule message and
// `scripts/dsl-inline-census.txt` own it, and a count written at this distance is exactly what went
// wrong: on 2026-08-24 this comment carried a number both owners contradicted, and it was found by a
// review rather than by anything mechanical. Recounting it would only reset how long it takes to go
// stale. What survives at this distance is the worked example, because a `file:line` in a checked-out
// tree does not drift the way a total does:
// `pkg/services/authz/zanzana/store/migration/migrator.go:264`.
//
// The veto is anchored to the PASSWORD slot rather than to the whole userinfo, which is the difference
// between this arm and the `${...}` one beside it: `http://%s:testpass@%s` spells a literal password
// after a formatted username, and that is still a leak.
//
// The verb SHAPE is `%` followed by a letter that is NOT a hex digit (`[g-zG-Z]`), and that class is
// load-bearing rather than a stylistic choice: percent-ENCODING is `%` plus exactly two hex digits, so
// `%` + a non-hex letter cannot be one, by construction. A wider shape (flags/width before the verb)
// would swallow `http://%75ser:%70ass@example.com` — astro's URL-decoding fixture, where `%70ass`
// decodes to a real `pass` and the line is a genuine finding of exactly the kind this rule exists for.
//
// This arm's veto rests on a WEAKER claim than the four beside it, and the difference is the reason the
// message states them separately. `${...}`, `{{...}}`, `process.env` and Rust's brace placeholders all
// mean the value is filled in somewhere else; a printf verb does not, because printf ARGUMENTS sit on
// the same line by construction:
//
//     dsn := fmt.Sprintf("postgres://%s:%s@%s/db", user, "S3cr3tPw9!", host)
//
// That line is vetoed here with its credential still on screen, and nothing downstream recovers it:
// `hardcoded-password` is `.java`-only, `hardcoded-secret` requires a secret NAME before the value and
// this argument is positional, `high-entropy-secret` needs a parser-projected secret-named binding, and
// `jwt-sign-literal-secret` needs `jwt.sign`. Measured over the corpus the gap costs nothing — the one
// instance that puts a literal in the arguments spells it `"password"`, which the placeholder arm
// vetoes anyway — but it is a gap, not an absence, and it is the thing to reopen if a real leak in this
// shape is ever reported. Reading the arguments is what would close it, and this arm deliberately does
// not: a veto that has to parse a call to know what it silenced is a different mechanism.

#[test]
fn go_sprintf_percent_verb_connection_string_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec-printf");
    dir.write(
        "src/db.go",
        "func connStr(user, pass, host, port, name string) string {\n\treturn fmt.Sprintf(\"postgresql://%s:%s@%s:%s/%s\", user, pass, host, port, name)\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn python_percent_format_connection_string_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec-printf");
    dir.write(
        "db/url.py",
        "URL = \"postgresql://%s:%s@%s/%s\" % (user, password, host, name)\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The under-detection boundary the verb shape must NOT cross, and the reason it refuses digits after
/// the `%`: astro `packages/internal-helpers/test/path.test.ts:133` writes
/// `http://%75ser:%70ass@example.com` to exercise URL decoding — `%70ass` IS the password `pass`,
/// spelled in the source, and a `%`-shape loose enough to read it as a format verb would erase a
/// finding this rule is exactly for.
#[test]
fn a_percent_encoded_userinfo_is_still_flagged() {
    let dir = TempDir::new("zzop-be-sec-printf");
    dir.write(
        "api/decode.test.ts",
        "const encoded = 'http://%75ser:%70ass@example.com';\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "conn-string-credentials");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

/// Second half of that boundary, and the reason the arm is anchored to the password slot rather than
/// to the userinfo at large: a formatted USERNAME does not launder a password spelled literally after
/// the colon — the same standing the host-slot case already had.
#[test]
fn a_percent_verb_username_does_not_launder_a_literal_password() {
    let dir = TempDir::new("zzop-be-sec-printf");
    dir.write(
        "src/client.go",
        "url := fmt.Sprintf(\"http://%s:testpass@%s/api/admin/stats\", login, addr)\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "conn-string-credentials");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn loopback_host_credential_is_not_flagged() {
    // immich dogfood (round 7): `postgres://postgres:postgres@localhost:5432/immich` in a dev
    // script — a credential that only answers on loopback is not a remotely usable leak.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/bin/sync.ts",
        "process.env.DB_URL = 'postgres://postgres:postgres@localhost:5432/immich';\nconst alt = \"redis://user:realish@127.0.0.1:6379\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn loopback_lookalike_host_still_fires() {
    // `localhost.evil.com` is NOT loopback — the veto must not match a prefix of a real host.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/db.ts",
        "export const url = \"postgres://svc:S3cr3tPw9@localhost.evil.com:5432/db\";\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "conn-string-credentials").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn placeholder_substring_password_is_not_flagged() {
    // immich dogfood (round 7): spec fixtures use `mypg:mypwd@myhost` — the placeholder word
    // (`pwd`) sits inside a longer token, so the veto matches placeholder words as substrings.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/services/backup.spec.ts",
        "const dbUrl = 'postgresql://mypg:mypwd@myhost:1234/myimmich?sslmode=require';\nconst two = \"amqp://svc:examplePass2@mq.internal:5672\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn product_default_password_is_not_flagged() {
    // immich dogfood (round 7): `postgres://postgres1:postgres2@database1:54320/immich` spec
    // fixture — a database product name (optionally digit-suffixed) in the password slot is a
    // default/metasyntactic credential, not a leaked secret. Deliberate tradeoff: a REAL
    // unchanged-default (`:root@`) is a weak-default problem, not a committed-secret leak —
    // out of this rule's scope.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/repositories/config.repository.spec.ts",
        "process.env.DB_URL = 'postgres://postgres1:postgres2@database1:54320/immich';\nconst r = \"redis://svc:redis@cache.internal:6379\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn connection_string_with_changeme_placeholder_word_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "export const url = \"postgres://user:changeme@host:5432/db\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn connection_string_with_password_placeholder_word_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "export const url = \"postgres://user:password@host:5432/db\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn conn_cred_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/db.ts",
        "// zzop-conn-string-credentials-ok: local docker-compose sample connection string, not a real credential\nexport const url = \"postgres://user:hunter2@host:5432/db\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "conn-string-credentials").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The placeholder veto used to be a NAME LIST, and a name list only knows the names someone typed
/// into it. Measured 2026-08-19 across 19 upstream checkouts: this rule produced 69 findings and all
/// 69 were documentation examples — `scott:tiger` 36 times (Oracle's demo account, which SQLAlchemy
/// writes into every dialect docstring), `u:p` 17, plus `foo:bar` and `user:pass`. Only the last was
/// on the list. The message meanwhile promised "placeholder-SHAPED passwords", which is what the
/// list was not.
///
/// So the veto gained a SHAPE alternative beside the names: a password slot that is nothing but a
/// short run of letters carries no entropy and is not a credential anyone can use. The names stay —
/// `changeme`, `passwd` and friends are longer than the shape admits — and both are pinned here
/// together with a real credential of the same length, because the only thing separating them is
/// that one has digits and punctuation in it.
#[test]
fn short_all_letter_passwords_are_placeholders_and_a_real_one_of_the_same_length_still_fires() {
    let dir = TempDir::new("zzop-be-sec-shape");
    dir.write(
        "docs/connecting.py",
        concat!(
            "URLS = [\n",
            "    \"mysql+mysqldb://scott:tiger@192.168.0.134/test\",\n",
            "    \"mssql+pyodbc://u:p@some_dsn\",\n",
            "    \"postgresql://foo:bar@hostspec/database\",\n",
            "    \"postgresql://admin:x7Kd$9q@db.internal:5432/app\",\n",
            "]\n"
        ),
    );
    let out = scan(&dir);
    let lines: Vec<u32> = hits(&out, "conn-string-credentials")
        .iter()
        .map(|h| h.line)
        .collect();
    assert_eq!(
        lines,
        vec![5],
        "only the entropy-bearing password is a credential: {:?}",
        out.findings
    );
}

/// The other side of the same line, kept so a later widening of the shape cannot quietly swallow it:
/// seven letters is past what the veto admits, because that is where "obvious placeholder" stops and
/// "someone's weak password" starts. `tiger^5HHH` is the real spelling that survived the corpus
/// sweep — a placeholder USERNAME does not launder a password that carries punctuation and digits.
#[test]
fn a_seven_letter_password_and_a_punctuated_placeholder_both_still_fire() {
    let dir = TempDir::new("zzop-be-sec-shape2");
    dir.write(
        "api/db.ts",
        concat!(
            "const a = \"postgres://svc:letmein@db.prod:5432/app\";\n",
            "const b = \"mssql+pyodbc://scott:tiger^5HHH@mssql2017:1433/test\";\n"
        ),
    );
    let out = scan(&dir);
    let lines: Vec<u32> = hits(&out, "conn-string-credentials")
        .iter()
        .map(|h| h.line)
        .collect();
    assert_eq!(lines, vec![1, 2], "{:?}", out.findings);
}
