// The re-export hop that keeps `dead.reexport-chain.decoy.ts` reachable. Also a control in its own right:
// a pure barrel file with no logic must not be reported by anything.
export { chainedHelper } from './dead.reexport-chain.decoy';
