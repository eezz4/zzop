# be-security/weak-password-hash (Python lane) — bad: hashlib's MD5 constructor on a line that also
# names the credential. good: sha256 (a strong digest, so the algorithm filter declines it), and the
# GENERIC constructor with a variable algorithm, which is the channel's never-guess case: the digest
# construction is real and witnessed, but the algorithm is not spelled at the site, so the site
# carries none and this rule stays silent rather than approximating.
import hashlib


def bad(password):
    return hashlib.md5(password.encode()).hexdigest()


def good_strong(password):
    return hashlib.sha256(password.encode()).hexdigest()


def good_unspelled_algorithm(password, algo):
    return hashlib.new(algo, password.encode()).hexdigest()
