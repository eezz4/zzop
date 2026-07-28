// DECOY for security/api-key-in-url. In scope: `.ts`, no require_file, not a test path. The rule's
// line_pattern wants a credential-named QUERY PARAMETER followed immediately by `=`. These URLs are plain
// string constants — never handed to a client — so no io consume is minted and no cross-layer count moves.
export const PAY_URL = 'https://vendor-alpha.example.net/v1/pay?amount=1200&currency=usd';
// `tokenType=` is not `token=`: the rule anchors the `=` directly after the credential word.
export const ITEMS_URL = 'https://vendor-beta.example.net/v1/items?tokenType=bearer&page=2';
export const PING_URL = 'https://vendor-gamma.example.net/v1/ping';
