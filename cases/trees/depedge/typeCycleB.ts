import type { TA } from './typeCycleA';
export interface TB {
  a: TA | null;
}
