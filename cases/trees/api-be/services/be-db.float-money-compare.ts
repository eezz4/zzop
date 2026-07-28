// be-db/float-money-compare — bad: strict equality on a float money value. good: compare integer cents.
export function bad(price: number) {
  return price === 9.99;
}

export function good(priceCents: number) {
  return priceCents === 999;
}
