// DECOY for egress/http-url-literal + code-hygiene/localhost-url-literal-committed. In scope: `.ts`, no
// require_file. Line 1 matches mixed-content's `'http://` line_pattern and is vetoed by its `w3.org`
// namespace arm; line 2 matches BOTH rules' patterns and is vetoed by mixed-content's `localhost` arm and
// by localhost-egress's env-fallback arm; line 3 is plain https.
export const SVG_NS = 'http://www.w3.org/2000/svg';
export const API_BASE = process.env.API_BASE ?? 'http://localhost:3000';
export const PROD_BASE = 'https://api.example.net/v1';
