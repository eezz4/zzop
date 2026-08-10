package com.example.svc;

// The Java lane of FOUR multi-language `security` rules. Each of these admits `.java` in its
// `file_pattern` and, until 2026-07-29, was only ever exercised on a TypeScript fixture — so the Java
// half of its regex was shipped unmeasured. The sibling `UnsafeController.java` covers the eleven
// Java-only rules; these four are the remainder, which is what took this pack's Java coverage to 15/15.
//
// Java is lexically parsed, so this need not compile. Every line below is a PLANTED defect; the `good`
// counterpart of each sits beside it so the fixture also shows the shape that must NOT fire.
public class CredentialsConfig {

  // security/hardcoded-secret — a secret-named field assigned a literal. The `good` form reads it from
  // the environment, which the rule's own exclude_pattern is built around.
  private static final String apiKey = "a7Fk29QmZx41Lp08Wd";
  private static final String goodApiKey = System.getenv("ZZOP_API_KEY");

  // security/conn-string-credentials — user:password embedded in a URI. The `good` form keeps the
  // credentials out of the string entirely.
  private static final String dsn = "postgresql://svcuser:Hf82kdQ1xZ@db.internal:5432/app";
  private static final String goodDsn = System.getenv("DATABASE_URL");

  // security/api-key-in-url — the key travels as a query parameter, where it lands in access logs,
  // proxies and browser history. The `good` form sends it as a header instead.
  private static final String endpoint = "https://vendor.example.com/v1/reports?api_key=8Kd02mZqXf41Lp";
  private static final String goodEndpoint = "https://vendor.example.com/v1/reports";

  // security/weak-password-hash — MD5 over a password. The `good` form names an adaptive hash.
  public String digestPassword(String password) throws Exception {
    return java.security.MessageDigest.getInstance("MD5").digest(password.getBytes()).toString();
  }

  public String goodDigestPassword(String password) {
    return org.springframework.security.crypto.bcrypt.BCrypt.hashpw(password, org.springframework.security.crypto.bcrypt.BCrypt.gensalt(12));
  }

  // security/high-entropy-secret — a secret-named field whose literal clears the rule's 80-bit Shannon
  // floor. Distinct from `hardcoded-secret` above and deliberately so: that rule judges the NAME/value
  // SHAPE, this one MEASURES the value, and `apiKey`'s shorter literal above trips the first while
  // staying under this one's floor. Having both in one file is what proves they are separate axes.
  private static final String sessionToken = "qV7mR2xL9tB4nH6kW8pD3sJ5gZ1cY0fA";
  private static final String goodSessionToken = System.getenv("ZZOP_SESSION_TOKEN");

  // security/bcrypt-cost-too-low — a single-digit cost factor. `goodDigestPassword` above is already the
  // negative control: its two-digit `gensalt(12)` must stay silent, so this pair also pins that the
  // rule reads the NUMBER rather than merely co-occurring with the word bcrypt.
  public String weakHashPassword(String password) {
    return org.springframework.security.crypto.bcrypt.BCrypt.hashpw(password, org.springframework.security.crypto.bcrypt.BCrypt.gensalt(4));
  }
}
