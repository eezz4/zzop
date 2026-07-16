use crate::{hits, scan, TempDir};

// --- private-key-committed ---

#[test]
fn rsa_private_key_pem_header_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "config/keys.ts",
        "export const key = `-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEAx1n...\n-----END RSA PRIVATE KEY-----`;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "private-key-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn unlabeled_pkcs8_private_key_pem_header_is_flagged() {
    // The `(RSA |EC |DSA |OPENSSH |ENCRYPTED |PGP )?` prefix group is optional — a bare PKCS8
    // "PRIVATE KEY" header (no algorithm prefix) must still fire.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "secrets/id_rsa.pem",
        "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQ...\n-----END PRIVATE KEY-----\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "private-key-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn openssh_private_key_header_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "deploy/config.yaml",
        "sshKey: |\n  -----BEGIN OPENSSH PRIVATE KEY-----\n  b3BlbnNzaC1rZXktdjEAAAAA...\n  -----END OPENSSH PRIVATE KEY-----\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "private-key-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn private_key_committed_in_a_test_fixture_path_is_still_flagged() {
    // Repo-content rule: an actual PEM header in a test fixture is still a committed key.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/__tests__/fixtures.test.ts",
        "export const testKey = `-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEAx1n...\n-----END RSA PRIVATE KEY-----`;\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "private-key-committed").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn private_key_header_generated_via_template_interpolation_is_not_flagged() {
    // exclude_pattern claim: an interpolation shape (`${`/`{{`) ON THE HEADER LINE itself reads as a
    // key-template generator, not a literal committed key.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "scripts/keygen-template.ts",
        "export const header = `-----BEGIN ${keyType} PRIVATE KEY-----`;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "private-key-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn private_key_ok_marker_above_the_header_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "config/keys.ts",
        "// private-key-ok: throwaway key generated only for this test, never used against a real service\nexport const key = `-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEAx1n...\n-----END RSA PRIVATE KEY-----`;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "private-key-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn non_pem_looking_dashes_are_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "docs/notes.ts",
        "export const divider = \"----- a plain divider, not a key -----\";\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "private-key-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn pem_header_mentioned_in_prose_without_a_key_body_is_not_flagged() {
    // Class fix (mono-hub 0.10.0 FP): an i18n/doc string that merely NAMES the PEM header — no
    // base64 key material accompanies it — is a non-key reading, so the header substring alone
    // must not fire. The line_pattern requires either the header at end-of-line (a dedicated key
    // line) or a base64 body after it; a header wrapped in a prose sentence satisfies neither.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/i18n/en.json",
        "{\n  \"guide\": \"Running keygen produces a `-----BEGIN PRIVATE KEY-----` block within a few seconds.\"\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "private-key-committed").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn single_line_json_embedded_private_key_with_base64_body_is_flagged() {
    // The complement of the prose fixture: a real key committed as a single JSON string (header,
    // an escaped `\n`, then the base64 body all on one physical line) must still fire — the
    // base64-body alternative of the line_pattern is what catches it.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "config/secrets.json",
        "{ \"privateKey\": \"-----BEGIN PRIVATE KEY-----\\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSj\\n-----END PRIVATE KEY-----\" }\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "private-key-committed").len(),
        1,
        "{:?}",
        out.findings
    );
}
