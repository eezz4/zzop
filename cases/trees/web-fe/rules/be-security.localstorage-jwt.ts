// be-security/localstorage-jwt — bad: a token written to localStorage (XSS-exfiltratable).
// good: keep the token in memory (no persistent, script-readable store).
let inMemoryToken = '';

export function bad(token: string) {
  localStorage.setItem('jwt', token);
}

export function good(token: string) {
  inMemoryToken = token;
  return inMemoryToken;
}
