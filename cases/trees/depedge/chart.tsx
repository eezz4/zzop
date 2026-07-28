// Loaded ONLY via a dynamic import() inside a dynamic()/lazy() wrapper. Gains fan-in via the
// dynamic-import dep edge (dynamic-import-edges-v1). Regression = code-split module dead-candidates FP.
export default function Chart(): null {
  return null;
}
