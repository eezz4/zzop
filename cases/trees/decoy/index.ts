// Entry barrel for the `decoy` tree — the corpus's whole-file precision axis (see ./README.md).
// It namespace-imports every control module so none of them reads as a dead-candidate or dead-export:
// unreferenced scaffolding makes those rules fire LEGITIMATELY, and the scaffolding would then score as a
// false positive against its own control.
//
// Two files are deliberately absent here, and that absence is the thing under test:
//   lib/dead.reexport-chain.decoy.ts  — reachable only through lib/reexports.ts
//   lib/dead.dynamic-import.decoy.ts  — reachable only through lib/dead.dynamic-loader.ts's `import(...)`
import * as browserNoDocumentWrite from './lib/browser.no-document-write.decoy';
import * as browserNoSystemDialogs from './lib/browser.no-system-dialogs.decoy';
import * as dbFloatMoneyCompare from './lib/db.float-money-compare.decoy';
import * as dbUnawaitedWrite from './lib/db.unawaited-write.decoy';
import * as dbUpdateDeleteNoWhere from './lib/db.update-delete-no-where.decoy';
import * as deadDynamicLoader from './lib/dead.dynamic-loader';
import * as egressMixedContent from './lib/egress.http-url-literal.decoy';
import * as reexports from './lib/reexports';
import * as reliabilityBodyLimitMissing from './lib/reliability.body-limit-missing.decoy';
import * as reliabilityIntervalNoClear from './lib/reliability.interval-no-clear.decoy';
import * as securityApiKeyInUrl from './lib/security.api-key-in-url.decoy';
import * as securityConnStringCredentials from './lib/security.conn-string-credentials.decoy';
import * as securityCorsWildcard from './lib/security.cors-wildcard.decoy';
import * as securityEvalDynamicCode from './lib/security.eval-dynamic-code.decoy';
import * as securityHardcodedSecret from './lib/security.hardcoded-secret.decoy';
import * as securityJwtNoneAlgorithm from './lib/security.jwt-none-algorithm.decoy';
import * as securityLocalstorageJwt from './lib/security.localstorage-jwt.decoy';
import * as securityRawQueryInterpolation from './lib/security.raw-query-unsafe-api.decoy';
import * as securityShellExecInterpolation from './lib/security.shell-exec-interpolation.decoy';
import * as securityTimingUnsafeCompare from './lib/security.timing-unsafe-compare.decoy';
import * as securityWeakPasswordHash from './lib/security.weak-password-hash.decoy';
import * as securityWeakTokenRandom from './lib/security.weak-token-random.decoy';
import * as sqlAppSideAggregation from './lib/sql.app-side-aggregation.decoy';
import * as sqlNoWhere from './lib/sql.no-where.decoy';
import * as sqlSelectStar from './lib/sql.select-star.decoy';
import * as typescriptAsCast from './lib/typescript.as-cast.decoy';
import * as typescriptParseIntNoRadix from './lib/typescript.parseint-no-radix.decoy';
import * as apiConsoleInBe from './api/reliability.console-in-be.decoy';
import * as apiNplus1 from './api/sql.nplus1.decoy';
import * as apiRaceConditionToctou from './api/sql.race-condition-toctou.decoy';
import * as servicesFetchNoTimeout from './services/reliability.fetch-no-timeout.decoy';
import * as servicesJsonParseNoTry from './services/reliability.json-parse-no-try.decoy';
import * as webSecretEnvInFe from './web/security.secret-env-in-fe.decoy';

export const registry = {
  browserNoDocumentWrite,
  browserNoSystemDialogs,
  dbFloatMoneyCompare,
  dbUnawaitedWrite,
  dbUpdateDeleteNoWhere,
  deadDynamicLoader,
  egressMixedContent,
  reexports,
  reliabilityBodyLimitMissing,
  reliabilityIntervalNoClear,
  securityApiKeyInUrl,
  securityConnStringCredentials,
  securityCorsWildcard,
  securityEvalDynamicCode,
  securityHardcodedSecret,
  securityJwtNoneAlgorithm,
  securityLocalstorageJwt,
  securityRawQueryInterpolation,
  securityShellExecInterpolation,
  securityTimingUnsafeCompare,
  securityWeakPasswordHash,
  securityWeakTokenRandom,
  sqlAppSideAggregation,
  sqlNoWhere,
  sqlSelectStar,
  typescriptAsCast,
  typescriptParseIntNoRadix,
  apiConsoleInBe,
  apiNplus1,
  apiRaceConditionToctou,
  servicesFetchNoTimeout,
  servicesJsonParseNoTry,
  webSecretEnvInFe,
};
