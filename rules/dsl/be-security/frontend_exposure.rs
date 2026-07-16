use crate::{hits, scan, TempDir};

// --- secret-env-in-fe ---

#[test]
fn server_only_secret_env_var_referenced_in_a_tsx_file_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/components/ApiKeyBanner.tsx",
        "export const key = process.env.SUPABASE_SERVICE_ROLE_KEY;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "secret-env-in-fe");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn public_env_var_referenced_in_a_tsx_file_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/components/PublicConfig.tsx",
        "export const apiUrl = process.env.NEXT_PUBLIC_API_URL;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "secret-env-in-fe").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- localstorage-jwt ---

#[test]
fn token_written_to_local_storage_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "web/auth.ts",
        "export function saveToken(token: string) {\n  localStorage.setItem(\"token\", token);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "localstorage-jwt");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn non_token_value_written_to_local_storage_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "web/prefs.ts",
        "export function saveTheme(theme: string) {\n  localStorage.setItem(\"theme\", theme);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "localstorage-jwt").is_empty(),
        "{:?}",
        out.findings
    );
}
