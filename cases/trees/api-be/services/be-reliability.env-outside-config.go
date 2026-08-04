// be-reliability/env-outside-config (Go lane) — bad: keyed process-environment reads outside the
// env-config module this corpus declares (config/env.ts, via zzop-attributes.json). good: reading an
// injected value. os.Environ() is deliberately NOT a read site — the producer names the two keyed
// idioms only, the same whole-environment boundary Python draws for bare os.environ.
package services

import "os"

func BadDsn() string {
	return os.Getenv("DATABASE_URL")
}

func BadPort() (string, bool) {
	return os.LookupEnv("PORT")
}

func Good(dsn string) string {
	return dsn
}
