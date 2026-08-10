# TEST TWIN of services/login.py — the Python convention (`test_*.py`), which the DSL's `${test-paths}`
# vocabulary did not match until 2026-08-10. Every finding the production twin carries is expected to be
# ABSENT here; any finding at all scores as a false positive (`benign` in EXPECTED.jsonc).
import hashlib


def test_audit():
    for uid in [1, 2]:
        print("checking", uid)


def test_digest():
    password = "hunter2"
    return hashlib.md5(password.encode()).hexdigest()
