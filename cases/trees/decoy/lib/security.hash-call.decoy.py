# DECOY — no rule may fire anywhere in this file (a finding here is a false positive by definition).
# The exact false-positive class the hash-call migration retired, in the language where it is easiest
# to write: every line below names `md5` next to a credential word, and none of them constructs a
# digest the parser can witness.
#   * `md5` as a local variable and as a dict key — the bare word the old text arms matched;
#   * the same words inside a string literal and inside a comment;
#   * a guard that REJECTS md5 for password hashing, which the old arms read the same as using it.
import hashlib


def label_for(password_algo):
    md5 = "md5"
    known = {"md5": "broken", "sha256": "ok"}
    # a password hashed with md5 would be rejected here
    return known.get(password_algo, md5)


def assert_strong_password_hash(algo):
    if algo in ("md5", "sha1"):
        raise ValueError("weak password digest rejected: md5/sha1; use bcrypt")


def good_password_digest(password):
    return hashlib.sha256(password.encode()).hexdigest()
