// Config-shaped literals. Each rule here is either a regex that never mentions a language at all, or
// one that reads a WIRE header name/value rather than any JS API (`security/cors-wildcard`,
// `security/csp-weak-or-disabled`) — which is why each one was widened rather than reimplemented.

/// egress/http-url-literal — a plain-http URL literal. The good form is https.
pub const REPORTS_BASE: &str = "http://reports.internal.example.com/v1";
pub const GOOD_REPORTS_BASE: &str = "https://reports.internal.example.com/v1";

/// code-hygiene/localhost-url-literal-committed — a loopback URL committed to source. Loopback is also what
/// keeps `egress/http-url-literal` off this same line.
pub const DEV_BASE: &str = "http://localhost:8080/health";

/// security/cors-wildcard — the wildcard `Access-Control-Allow-Origin` header value. The good form
/// names one allow-listed origin.
pub fn cors_headers() -> Vec<(&'static str, &'static str)> {
    vec![("Access-Control-Allow-Origin", "*")]
}

pub fn good_cors_headers() -> Vec<(&'static str, &'static str)> {
    vec![("Access-Control-Allow-Origin", "https://app.example.com")]
}

/// reliability/debug-true-committed — a debug flag baked into a committed default. The good form reads
/// the environment; the struct FIELD declaration is not a value assignment and does not fire either.
pub struct Settings {
    pub debug: bool,
}

pub fn settings() -> Settings {
    Settings { debug: true }
}

pub fn good_settings() -> Settings {
    Settings { debug: std::env::var("ZZOP_DEBUG").is_ok() }
}

/// security/csp-weak-or-disabled — wire-header CSP VALUES (widened to `.rs` 2026-08-02). The
/// `unsafe-inline` and wildcard `default-src` arms read the header value, so they judge Rust exactly
/// as they judge JS; Helmet's `contentSecurityPolicy: false` arm is a JS option key that cannot occur
/// here — arm-level silence, not rule-level (the rule's message says so). The good form directly below
/// each bad one is a strict self-scoped policy on the same header, so the fixture pins the shape that
/// must NOT fire as well as the one that must.
pub fn csp_unsafe_inline_headers() -> Vec<(&'static str, &'static str)> {
    vec![("Content-Security-Policy", "default-src 'self'; script-src 'unsafe-inline'")]
}

pub fn good_csp_headers() -> Vec<(&'static str, &'static str)> {
    vec![("Content-Security-Policy", "default-src 'self'; script-src 'self'")]
}

pub fn csp_wildcard_headers() -> Vec<(&'static str, &'static str)> {
    vec![("Content-Security-Policy", "default-src * 'unsafe-eval'")]
}

pub fn good_csp_wildcard_headers() -> Vec<(&'static str, &'static str)> {
    vec![("Content-Security-Policy", "default-src 'self' https://cdn.example.net")]
}

/// security/cors-wildcard — the Rust idiom arms (2026-08-03): tower-http's `Any`, actix's
/// `allow_any_origin`, and the typed header constant beside a `"*"` value. Each good form is the
/// nearest benign lookalike its arm has to keep distinguishing: an exact origin through the SAME
/// method, the `allowed_origin`/`allow_any_*` family members that never touch the origin, and the
/// same constant beside an allow-listed value.
pub fn cors_layer() -> CorsLayer {
    CorsLayer::new().allow_origin(Any)
}

pub fn good_cors_layer() -> CorsLayer {
    CorsLayer::new().allow_origin(AllowOrigin::exact("https://app.example.com".parse().unwrap()))
}

pub fn actix_cors() -> Cors {
    Cors::default().allow_any_origin()
}

pub fn good_actix_cors() -> Cors {
    Cors::default().allowed_origin("https://app.example.com").allow_any_method().allow_any_header()
}

pub fn cors_header_pair() -> (HeaderName, HeaderValue) {
    (header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"))
}

pub fn good_cors_header_pair() -> (HeaderName, HeaderValue) {
    (header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("https://app.example.com"))
}
