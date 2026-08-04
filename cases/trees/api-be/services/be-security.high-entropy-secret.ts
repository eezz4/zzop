// be-security/high-entropy-secret — bad: a passphrase-style credential (97.9 total Shannon bits) and a
// no-digit base64url token (108.0 bits) bound to secret-named bindings — both were structurally silent
// under hardcoded-secret's value-shape veto. good: an identifier-shaped value (41.4 bits) and a
// placeholder-word value (75.7 bits), both under the measured 80-bit floor.
export const badPassword = 'correct-horse-battery-staple';
export const badToken = 'kqZXvWyBpNdRtGmHsJfL-qwe';

export const goodToken = 'refresh-token';
export const goodSecret = 'PlaceholderSecretValue';

// bad: a boundary-adjacent name — `latestToken` EMBEDS "test" (la-TEST-…) with no name boundary, so the
// boundary-anchored mock-name veto must not silence it (the pre-repair substring veto did).
export const latestToken = 'trombone_ravine_wallet_ember';
