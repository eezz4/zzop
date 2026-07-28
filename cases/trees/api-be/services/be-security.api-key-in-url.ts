// be-security/api-key-in-url — bad: a secret carried in the URL query string. good: in an Authorization
// header instead.
export function bad(key: string) {
  return 'https://api.example.com/data?api_key=' + key;
}

export function good(key: string) {
  return { url: 'https://api.example.com/data', headers: { Authorization: 'Bearer ' + key } };
}
