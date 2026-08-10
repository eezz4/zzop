# security/weak-crypto (Python lane) — MUST FIRE on the hashlib.md5 line (EXPECTED py:15). See the
# TypeScript sibling (be-security.weak-crypto.ts) for the history: written as a labeled gap while
# the rule read Java only, promoted to a scored expectation on 2026-08-09 when the six-language
# `hash-call` migration landed (this lane's algorithm-named-constructor spelling included).
#
# bad: MD5 over an opaque API token, to key a rate-limit bucket without storing the raw value. The
# fingerprint is short-lived and never a credential itself, so `security/weak-password-hash` — which
# requires a credential word on the call line — stays silent; weak-crypto is the rule that judges it.
# MD5 is still wrong here: a collision lets two distinct callers share one rate-limit bucket.
# good: SHA-256, the same construction with a strong digest — must stay silent.
import hashlib


def bucket_fingerprint(raw_token: str) -> str:
    return hashlib.md5(raw_token.encode()).hexdigest()[:16]


def good_bucket_fingerprint(raw_token: str) -> str:
    return hashlib.sha256(raw_token.encode()).hexdigest()[:16]
