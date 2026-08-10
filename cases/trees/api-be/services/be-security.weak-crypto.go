// security/weak-crypto (Go lane) — MUST FIRE on the md5.Sum line (EXPECTED go:20). See the
// TypeScript sibling (be-security.weak-crypto.ts) for the history: written as a labeled gap while
// the rule read Java only, promoted to a scored expectation on 2026-08-09 when the six-language
// `hash-call` migration landed.
//
// bad: MD5 over a response body to build an ETag. Go spells this differently from every other
// language in the corpus — `md5.Sum` on a byte slice, with the algorithm in the PACKAGE name rather
// than in a string argument — which is the detail a migration written against the Java spelling was
// most likely to miss; the parser's `hash-call` channel sees it because it reads the call site, not
// a transliterated regex. good: SHA-256, the same construction with a strong digest — stays silent.
package services

import (
	"crypto/md5"
	"crypto/sha256"
	"encoding/hex"
)

func ETagFor(body []byte) string {
	sum := md5.Sum(body)
	return `W/"` + hex.EncodeToString(sum[:]) + `"`
}

func GoodETagFor(body []byte) string {
	sum := sha256.Sum256(body)
	return `W/"` + hex.EncodeToString(sum[:]) + `"`
}
