import { a } from './circularA';

const usesA = a;
export const b = 2 + (usesA ? 0 : 0);
