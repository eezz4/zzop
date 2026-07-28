// DECOY for security/localstorage-jwt. In scope: `.ts`, no require_file. The rule's line_pattern is
// `localStorage.setItem(... token|jwt|access_token ...)`; these probe both halves of it — a localStorage
// write of a non-credential, and credential handling that is not a localStorage SET.
export declare const localStorage: { setItem(k: string, v: string): void; removeItem(k: string): void };
export declare const sessionStorage: { setItem(k: string, v: string): void };

export function rememberTheme(theme: string): void {
  localStorage.setItem('theme', theme);
}

export function stashAccessToken(value: string): void {
  sessionStorage.setItem('accessToken', value);
}

export function forgetToken(): void {
  localStorage.removeItem('token');
}
