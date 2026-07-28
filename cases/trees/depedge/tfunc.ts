// A type re-exported through TWO barrels and consumed as a type. Gains fan-in via the type-only
// re-export chain (in the dep graph, excluded from circular). Regression = dead-candidates FALSE POSITIVE.
export type TFunc = (k: string) => string;
