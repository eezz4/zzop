// be-reliability/env-outside-config (Java lane) — bad: System.getenv outside the declared env-config
// module (config/env.ts, via zzop-attributes.json). good: an injected value. The console fixtures for
// Java live in the java-svc tree (its services/ path); this file exists in api-be because only this
// tree declares the env-config-module attribute the rule is gated on.
class BeReliabilityEnvOutsideConfig {
  String badDsn() {
    return System.getenv("DATABASE_URL");
  }

  String good(String dsn) {
    return dsn;
  }
}
