use crate::{assert_landing_precedes_imperative, hits, scan, TempDir};

/// The DOCTYPE landing for `xxe-no-guard`, spliced ahead of the `disallow-doctype-decl` imperative.
///
/// WHY (`1.architecture/rules/rule-quality.md` §27 leg 3, §37). The message already carried the
/// CONDITIONAL exit — "if DOCTYPEs are genuinely required, set the two entity features instead" — but
/// never the consequence that selects it. A reader who does not already know what their parser is fed
/// has no way to tell whether the condition applies to them, so the exit reads as an aside and the
/// first setting reads as the answer. It is not an aside: `disallow-doctype-decl` makes the parser
/// raise a fatal error on the whole DOCUMENT, so an ingest path that accepts XHTML, a DTD-validated
/// industry format, or a feed declaring its own entities starts rejecting ONE HUNDRED PERCENT of them.
///
/// NOT A DISQUALIFIER. A parser reading DOCTYPE-carrying documents is exactly the one this finding is
/// most about — external entity resolution is only reachable through a DOCTYPE. The finding stands;
/// what changes is which of the two settings the reader is in a position to choose.
///
/// POSITION, not presence: the invalidation probe is to move this constant behind the imperative with
/// every token still spelled exactly once.
const DOCTYPE_REJECTION_LANDING: &str = "`disallow-doctype-decl` REJECTS THE DOCUMENT, NOT JUST THE ENTITY: with it set the parser raises a fatal `SAXParseException` on ANY input that carries a `<!DOCTYPE ...>`, benign ones included";

/// The trust-store landing for `trust-all-tls`, spliced ahead of the "use the default verifier"
/// imperative.
///
/// WHY. This rule shipped at 331 characters with no caveat of any kind, and its remedy is one of the
/// few in this pack that can take a working integration down on the first request after deploy. The
/// trust-all is nearly always LOAD-BEARING — it is there because the peer presents a private-CA cert,
/// a self-signed staging host, an IP with no matching SAN, or a TLS-inspecting proxy — so restoring
/// the default verifier converts every call to that peer into a handshake exception. Nothing about
/// that is visible at build time.
///
/// NOT A DISQUALIFIER, and the distinction matters more here than anywhere else in this file: a peer
/// with a private CA is precisely the case where the finding is RIGHT and the naive fix is wrong. The
/// landing therefore names the fix that keeps the call working (pin that CA into a `KeyStore`) rather
/// than offering the reader a reason to leave the trust-all in place.
///
/// POSITION, not presence: the invalidation probe is to move this constant behind the imperative with
/// every token still spelled exactly once.
const TRUST_STORE_LANDING: &str = "RESTORING VERIFICATION IS NOT A NO-OP WHERE THE TRUST-ALL WAS LOAD-BEARING, AND IT FAILS AT RUNTIME RATHER THAN AT BUILD";

// --- xxe-no-guard (Java) ---

/// POSITION pin on a DELIVERED finding (§27 leg 3): what the prescribed setting costs is reached
/// before the setting itself.
#[test]
fn the_doctype_landing_precedes_the_disallow_doctype_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/XmlParser.java",
        "public class XmlParser {\n    public Document parse(InputStream in) throws Exception {\n        DocumentBuilderFactory factory = DocumentBuilderFactory.newInstance();\n        DocumentBuilder builder = factory.newDocumentBuilder();\n        return builder.parse(in);\n    }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "xxe-no-guard");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "xxe-no-guard",
        &h[0].message,
        DOCTYPE_REJECTION_LANDING,
        "Set `disallow-doctype-decl` to `true`;",
    );
    // The landing exists to make the message's OWN conditional exit selectable. Both halves have to
    // stay: the population that breaks, and the instruction to go look at it.
    for needle in [
        "starts rejecting one hundred percent of them",
        "Look at what this parser is actually fed",
        "If DOCTYPEs are genuinely required",
    ] {
        assert!(
            h[0].message.contains(needle),
            "security/xxe-no-guard: the landing lost {needle:?}: {}",
            h[0].message
        );
    }
}

#[test]
fn document_builder_factory_with_no_guard_in_the_method_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/XmlParser.java",
        "public class XmlParser {\n    public Document parse(InputStream in) throws Exception {\n        DocumentBuilderFactory factory = DocumentBuilderFactory.newInstance();\n        DocumentBuilder builder = factory.newDocumentBuilder();\n        return builder.parse(in);\n    }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "xxe-no-guard");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn document_builder_factory_with_disallow_doctype_decl_guard_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/XmlParser.java",
        "public class XmlParser {\n    public Document parse(InputStream in) throws Exception {\n        DocumentBuilderFactory factory = DocumentBuilderFactory.newInstance();\n        factory.setFeature(\"http://apache.org/xml/features/disallow-doctype-decl\", true);\n        DocumentBuilder builder = factory.newDocumentBuilder();\n        return builder.parse(in);\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "xxe-no-guard").is_empty(), "{:?}", out.findings);
}

#[test]
fn feature_secure_processing_alone_no_longer_suffices_and_is_now_flagged() {
    // Per OWASP, FEATURE_SECURE_PROCESSING alone does NOT disable external entity resolution — the
    // matcher's `absent` veto list used to treat it as a sufficient guard on its own (a single combined
    // "disallow-doctype-decl|FEATURE_SECURE_PROCESSING" entry); now only disallow-doctype-decl=true or
    // both external-entities-false vetoes, so FSP-alone must fire.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/XmlParser.java",
        "public class XmlParser {\n    public Document parse(InputStream in) throws Exception {\n        DocumentBuilderFactory factory = DocumentBuilderFactory.newInstance();\n        factory.setFeature(XMLConstants.FEATURE_SECURE_PROCESSING, true);\n        DocumentBuilder builder = factory.newDocumentBuilder();\n        return builder.parse(in);\n    }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "xxe-no-guard");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn document_builder_factory_with_both_external_entities_disabled_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/XmlParser.java",
        "public class XmlParser {\n    public Document parse(InputStream in) throws Exception {\n        DocumentBuilderFactory factory = DocumentBuilderFactory.newInstance();\n        factory.setFeature(\"http://xml.org/sax/features/external-general-entities\", false);\n        factory.setFeature(\"http://xml.org/sax/features/external-parameter-entities\", false);\n        DocumentBuilder builder = factory.newDocumentBuilder();\n        return builder.parse(in);\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "xxe-no-guard").is_empty(), "{:?}", out.findings);
}

#[test]
fn document_builder_factory_with_only_external_general_entities_disabled_is_not_flagged() {
    // Documents the matcher's actual (intentionally disclosed in the message) OR semantics: each
    // `absent` entry vetoes independently, so a single recognized guard line is enough even though the
    // message recommends setting BOTH external-general-entities and external-parameter-entities.
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/XmlParser.java",
        "public class XmlParser {\n    public Document parse(InputStream in) throws Exception {\n        DocumentBuilderFactory factory = DocumentBuilderFactory.newInstance();\n        factory.setFeature(\"http://xml.org/sax/features/external-general-entities\", false);\n        DocumentBuilder builder = factory.newDocumentBuilder();\n        return builder.parse(in);\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "xxe-no-guard").is_empty(), "{:?}", out.findings);
}

#[test]
fn xxe_ok_marker_in_the_method_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/XmlParser.java",
        "public class XmlParser {\n    public Document parse(InputStream in) throws Exception {\n        // zzop-xxe-no-guard-ok: guard applied via a shared factory helper not visible in this method\n        DocumentBuilderFactory factory = DocumentBuilderFactory.newInstance();\n        DocumentBuilder builder = factory.newDocumentBuilder();\n        return builder.parse(in);\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "xxe-no-guard").is_empty(), "{:?}", out.findings);
}

// --- unsafe-deserialization (Java) ---

#[test]
fn object_input_stream_read_object_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/Loader.java",
        "public class Loader {\n    public Object load(byte[] data) throws Exception {\n        ObjectInputStream ois = new ObjectInputStream(new ByteArrayInputStream(data));\n        return ois.readObject();\n    }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "unsafe-deserialization");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn json_deserialization_with_no_object_input_stream_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/Loader.java",
        "public class Loader {\n    public Object load(String json) {\n        return objectMapper.readValue(json, Object.class);\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unsafe-deserialization").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- java-path-traversal (Java) ---

#[test]
fn new_file_built_from_a_request_parameter_in_the_same_method_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/FileController.java",
        "public class FileController {\n    public void download(HttpServletRequest request) throws IOException {\n        String filename = request.getParameter(\"file\");\n        File file = new File(\"/uploads/\" + filename);\n    }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "java-path-traversal");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn new_file_with_a_fixed_path_and_no_request_parameter_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/FileController.java",
        "public class FileController {\n    public void download() throws IOException {\n        File file = new File(\"/uploads/report.pdf\");\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "java-path-traversal").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- weak-random (Java) ---

#[test]
fn new_random_with_token_keyword_before_it_on_the_line_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/TokenGenerator.java",
        "public class TokenGenerator {\n    public String makeToken() {\n        String token = String.valueOf(new Random().nextLong());\n        return token;\n    }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "weak-random");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn new_random_with_session_keyword_after_it_on_the_line_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/SessionUtil.java",
        "public class SessionUtil {\n    public String makeSessionId() {\n        return new Random().nextLong() + \"-session\";\n    }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "weak-random").len(), 1, "{:?}", out.findings);
}

#[test]
fn new_random_with_no_security_keyword_on_the_line_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/DiceRoller.java",
        "public class DiceRoller {\n    public int roll() {\n        return new Random().nextInt(6) + 1;\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "weak-random").is_empty(), "{:?}", out.findings);
}

// --- trust-all-tls (Java) ---

#[test]
fn trust_all_certs_class_instantiation_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/InsecureSslContext.java",
        "public class InsecureSslContext {\n    public X509TrustManager trustAllCerts = new TrustAllCerts();\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "trust-all-tls");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn allow_all_hostname_verifier_constant_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/HttpClientConfig.java",
        "public class HttpClientConfig {\n    public void configure(HttpClient client) {\n        client.setHostnameVerifier(SSLConnectionSocketFactory.ALLOW_ALL_HOSTNAME_VERIFIER);\n    }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "trust-all-tls").len(), 1, "{:?}", out.findings);
}

#[test]
fn hostname_verifier_lambda_always_returning_true_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/HttpClientConfig.java",
        "public class HttpClientConfig {\n    public void configure(HttpsURLConnection conn) {\n        conn.setHostnameVerifier((hostname, session) -> true);\n    }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "trust-all-tls").len(), 1, "{:?}", out.findings);
}

#[test]
fn hostname_verifier_using_the_default_implementation_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/HttpClientConfig.java",
        "public class HttpClientConfig {\n    public void configure(HttpsURLConnection conn) {\n        conn.setHostnameVerifier(HttpsURLConnection.getDefaultHostnameVerifier());\n    }\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "trust-all-tls").is_empty(), "{:?}", out.findings);
}

/// POSITION pin on a DELIVERED finding (§27 leg 3): what restoring verification costs is reached
/// before the instruction to restore it.
#[test]
fn the_trust_store_landing_precedes_the_default_verifier_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/InsecureSslContext.java",
        "public class InsecureSslContext {\n    public X509TrustManager trustAllCerts = new TrustAllCerts();\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "trust-all-tls");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "trust-all-tls",
        &h[0].message,
        TRUST_STORE_LANDING,
        "Use the default (or a properly validating custom) trust manager/verifier.",
    );
    // The exit is the half that keeps this from reading as "leave it alone": a private chain is
    // TRUSTED, not trusted-blindly, and the reader is told which artifact does that.
    for needle in [
        "`SSLHandshakeException`/`CertificateException`",
        "load that CA into a `KeyStore`",
        "still detects a man in the middle",
    ] {
        assert!(
            h[0].message.contains(needle),
            "security/trust-all-tls: the landing lost {needle:?}: {}",
            h[0].message
        );
    }
}

#[test]
fn trust_all_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/InsecureSslContext.java",
        "public class InsecureSslContext {\n    // zzop-trust-all-tls-ok: used only in a local dev test harness against a self-signed cert\n    public X509TrustManager trustAllCerts = new TrustAllCerts();\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "trust-all-tls").is_empty(), "{:?}", out.findings);
}
