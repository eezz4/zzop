// be-reliability/env-outside-config (C# lane) — bad: a keyed environment read outside the declared
// env-config module (config/env.ts, via zzop-attributes.json). good: an injected value, plus the
// whole-environment GetEnvironmentVariables() (plural), which the producer deliberately does not
// treat as a keyed read site.
class BeReliabilityEnvOutsideConfig {
  string BadDsn() {
    return Environment.GetEnvironmentVariable("DATABASE_URL");
  }

  string Good(string dsn) {
    return dsn;
  }

  System.Collections.IDictionary GoodBulk() {
    return Environment.GetEnvironmentVariables();
  }
}
