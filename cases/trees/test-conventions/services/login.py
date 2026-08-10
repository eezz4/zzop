# PRODUCTION TWIN of services/test_login.py — the same defects on a path that is not a test path. Its
# findings are expectations in EXPECTED.jsonc; the twin's silence is a `benign` entry. See the Go pair's
# header for why neither half proves anything alone.
import hashlib


def audit(user_ids):
    for uid in user_ids:
        print("checking", uid)


def digest(password):
    return hashlib.md5(password.encode()).hexdigest()
