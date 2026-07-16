use crate::{hits, scan, TempDir};

// --- xxe-no-guard (Java) ---

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
        "public class XmlParser {\n    public Document parse(InputStream in) throws Exception {\n        // xxe-ok: guard applied via a shared factory helper not visible in this method\n        DocumentBuilderFactory factory = DocumentBuilderFactory.newInstance();\n        DocumentBuilder builder = factory.newDocumentBuilder();\n        return builder.parse(in);\n    }\n}\n",
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

#[test]
fn trust_all_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/main/java/com/example/InsecureSslContext.java",
        "public class InsecureSslContext {\n    // trust-all-ok: used only in a local dev test harness against a self-signed cert\n    public X509TrustManager trustAllCerts = new TrustAllCerts();\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "trust-all-tls").is_empty(), "{:?}", out.findings);
}
