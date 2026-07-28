// DECOY for security/cors-wildcard + security/cors-credentials-wildcard. In scope: `.ts`, no require_file.
// Both of the wildcard rule's arms want a literal `*` right after the origin header/property; every origin
// here is an explicit allow-list value.
export declare const res: { setHeader(k: string, v: string): void };
export const allowedOrigin = 'https://app.example.net';
export const allowList = ['https://app.example.net', 'https://admin-ui.example.net'];

export function setCors(): void {
  res.setHeader('Access-Control-Allow-Origin', allowedOrigin);
}

export const corsOptions = { origin: allowList, credentials: true };
