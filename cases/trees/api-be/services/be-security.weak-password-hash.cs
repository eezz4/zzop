// be-security/weak-password-hash (C# lane) — bad: the SHA1 factory, whose TYPE names the algorithm,
// on a line that also names the credential. good: SHA256 (strong), and the generic factory with a
// variable algorithm — the never-guess case, where the construction is witnessed but the algorithm is
// not spelled, so the site carries none and this rule says nothing.
class BeSecurityWeakPasswordHash {
  byte[] Bad(byte[] password) {
    return SHA1.Create().ComputeHash(password);
  }

  byte[] GoodStrong(byte[] password) {
    return SHA256.Create().ComputeHash(password);
  }

  byte[] GoodUnspelledAlgorithm(byte[] password, string algo) {
    return HashAlgorithm.Create(algo).ComputeHash(password);
  }
}
