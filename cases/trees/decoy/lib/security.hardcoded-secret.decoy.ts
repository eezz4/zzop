// DECOY for security/hardcoded-secret. In scope: `.ts` matches file_pattern and the rule has NO
// require_file and NO file_exclude, so it evaluates every line here. Each line trips the rule's
// `assignment` arm on the identifier and is then vetoed by a different arm of its exclude_pattern, or
// misses the 8-character literal floor. All values are synthetic.
export const token = 'REPLACE_ME_TOKEN';
export const password = 'example-value-here';
export const secret = 'PlaceholderSecretValue';
export const apikey = 'short';
export const apiKey = loadKey();
export const notLoadBearing = 'abcdefghij123456';
function loadKey(): string {
  return '';
}
