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
