// DECOY for db/float-money-compare. In scope: `.ts`, no require_file. Both of the rule's arms want a
// money-named identifier compared with `==`/`===`/`!==` against a DECIMAL literal. Integer minor units,
// an epsilon comparison, and a relational operator are the three correct forms and match neither arm.
export function isFlatFee(amountCents: number): boolean {
  return amountCents === 1999;
}

export function nearTarget(price: number, target: number): boolean {
  return Math.abs(price - target) < 0.005;
}

export function isFunded(balance: number): boolean {
  return balance >= 10.5;
}
