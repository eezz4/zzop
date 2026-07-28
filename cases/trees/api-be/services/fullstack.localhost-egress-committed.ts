// fullstack/localhost-egress-committed — bad: a committed localhost/private endpoint (breaks off the dev
// machine). good: a configurable/public URL.
export const bad = 'http://localhost:5432/orders';

export const good = 'https://orders.example.com/api';
