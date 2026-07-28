// Consumes TFunc through the 2-level type-only barrel chain (barrelOuter -> barrelInner -> tfunc).
import type { TFunc } from './barrelOuter';
export const useT = (f: TFunc): string => f('x');
