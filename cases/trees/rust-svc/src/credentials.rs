// The Rust lane of this pack's repo-content credential rules, plus `security/weak-password-hash`.
// Every bad-side item below is a PLANTED defect; its `good` counterpart sits directly beside it, so the
// fixture pins the shape that must NOT fire as well as the one that must. Rust is parsed by `syn`, so
// this file has to be syntactically valid: a parse failure would empty the symbol list, and a tree with
// no symbols answers byte-identically to a clean one.

/// security/hardcoded-secret, `assignment` arm — an untyped `let` whose name is secret-shaped and whose
/// value is a literal. The good form reads the process environment, which the arm structurally cannot
/// match because a call, not a quote, follows the `=`.
pub fn api_key() -> String {
    let api_key = "a7Fk29QmZx41Lp08Wd";
    api_key.to_string()
}

pub fn api_key_from_env() -> String {
    let api_key = std::env::var("ZZOP_API_KEY").unwrap_or_default();
    api_key
}

/// security/hardcoded-secret, `rust-str-const` arm — the typed form the `assignment` arm cannot see (a type sits between the name and the `=`). Good twin on the next line: its NAME (`SECRET`) still matches the trigger, but its identifier-shaped VALUE (an env-var name — what you look a secret up BY) is exactly what the value-shape veto kills; a `GOOD_`-prefixed name would dodge the trigger instead and prove nothing about the veto.
/// (EXPECTED pins these anchors by line number, so the twins share lines instead of paragraphs here.)
pub const API_KEY: &str = "a7Fk29QmZx41Lp08Wd";
pub const SECRET: &str = "ZZOP_SIGNING_SECRET"; // good twin — veto-killed identifier-shaped value
/// security/conn-string-credentials — `user:password` embedded in a URI. Good twin below: INTERPOLATED userinfo (`{user}:{password}`) puts no credential at rest — the shape the rule's interpolation veto exists to pass.
pub const DSN: &str = "postgresql://svcuser:Hf82kdQ1xZ@db.internal:5432/app";
pub fn good_dsn(user: &str, password: &str) -> String { format!("postgresql://{user}:{password}@db.internal:5432/app") }

/// security/api-key-in-url — the credential travels as a query parameter (it lands in access logs, proxies and browser history). The good form sends it as a header instead.
pub const REPORTS_URL: &str = "https://vendor.example.com/v1/reports?api_key=8Kd02mZqXf41Lp";
pub const GOOD_REPORTS_URL: &str = "https://vendor.example.com/v1/reports";

/// security/private-key-committed — a PEM header carrying its base64 key body. The good line names the
/// same header in prose with no key material after it, which the matcher deliberately does not flag.
pub const SIGNING_KEY: &str = "-----BEGIN RSA PRIVATE KEY-----MIIEowIBAAKCAQEAx0hK9rQ2vWpNsT4u";
pub const KEY_HINT: &str = "paste the -----BEGIN RSA PRIVATE KEY----- block here";

/// security/weak-password-hash — MD5 over a password. The good form names an adaptive hash at its
/// library default cost, which the `low-bcrypt-rounds` arm (a single-digit literal) does not match.
pub fn digest_password(password: &str) -> String {
    let digest = md5::compute(password.as_bytes());
    format!("{:x}", digest)
}

pub fn good_digest_password(password: &str) -> String {
    bcrypt::hash(password, bcrypt::DEFAULT_COST).unwrap_or_default()
}
