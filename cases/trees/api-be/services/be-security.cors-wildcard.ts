// be-security/cors-wildcard — bad: origin '*'. good: an explicit allowed origin.
export const bad = { origin: '*' };

export const good = { origin: 'https://app.example.com' };
