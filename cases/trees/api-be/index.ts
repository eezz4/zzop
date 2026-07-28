// Entry barrel (generated; keep in sync with api/ + services/). Namespace-imports every rule module.
import * as beReliabilityFetchNoTimeout from "./api/be-reliability.fetch-no-timeout";
import * as httpDevPathNoGuardHint from "./api/http.dev-path-no-guard-hint";
import * as httpGetRouteNoCacheMarker from "./api/http.get-route-no-cache-marker";
import * as httpProtectedPathNoAuthEvidence from "./api/http.protected-path-no-auth-evidence";
import * as sqlCountInLoop from "./api/sql.count-in-loop";
import * as sqlNplus1 from "./api/sql.nplus1";
import * as sqlRaceConditionToctou from "./api/sql.race-condition-toctou";
import * as beDbClientPerRequest from "./services/be-db.client-per-request";
import * as beDbEmptyCatchOnWrite from "./services/be-db.empty-catch-on-write";
import * as beDbExternalCallInTx from "./services/be-db.external-call-in-tx";
import * as beDbFloatMoneyCompare from "./services/be-db.float-money-compare";
import * as beDbPaginationNoOrderby from "./services/be-db.pagination-no-orderby";
import * as beDbUnawaitedWrite from "./services/be-db.unawaited-write";
import * as beDbUnboundedUserLimit from "./services/be-db.unbounded-user-limit";
import * as beDbUpdateDeleteNoWhere from "./services/be-db.update-delete-no-where";
import * as beReliabilityAsyncRouteNoCatch from "./services/be-reliability.async-route-no-catch";
import * as beReliabilityAwaitInMap from "./services/be-reliability.await-in-map";
import * as beReliabilityBodyLimitMissing from "./services/be-reliability.body-limit-missing";
import * as beReliabilityConsoleInBe from "./services/be-reliability.console-in-be";
import * as beReliabilityDebugTrueCommitted from "./services/be-reliability.debug-true-committed";
import * as beReliabilityEnvNonnullAssert from "./services/be-reliability.env-nonnull-assert";
import * as beReliabilityEnvOutsideConfig from "./services/be-reliability.env-outside-config";
import * as beReliabilityIntervalNoClear from "./services/be-reliability.interval-no-clear";
import * as beReliabilityJsonParseNoTry from "./services/be-reliability.json-parse-no-try";
import * as beReliabilityProcessExitInLib from "./services/be-reliability.process-exit-in-lib";
import * as beReliabilityPromiseAllWrites from "./services/be-reliability.promise-all-writes";
import * as beReliabilitySyncFsInHandler from "./services/be-reliability.sync-fs-in-handler";
import * as beSecurityApiKeyInUrl from "./services/be-security.api-key-in-url";
import * as beSecurityCorsCredentialsWildcard from "./services/be-security.cors-credentials-wildcard";
import * as beSecurityCorsWildcard from "./services/be-security.cors-wildcard";
import * as beSecurityErrorLeakToClient from "./services/be-security.error-leak-to-client";
import * as beSecurityHardcodedSecret from "./services/be-security.hardcoded-secret";
import * as beSecurityInsecureCookie from "./services/be-security.insecure-cookie";
import * as beSecurityJwtNoExpiry from "./services/be-security.jwt-no-expiry";
import * as beSecurityMassAssignment from "./services/be-security.mass-assignment";
import * as beSecurityOpenRedirect from "./services/be-security.open-redirect";
import * as beSecurityPathTraversal from "./services/be-security.path-traversal";
import * as beSecurityRawQueryInterpolation from "./services/be-security.raw-query-interpolation";
import * as beSecuritySsrfUserUrl from "./services/be-security.ssrf-user-url";
import * as beSecurityTimingUnsafeCompare from "./services/be-security.timing-unsafe-compare";
import * as beSecurityWeakPasswordHash from "./services/be-security.weak-password-hash";
import * as beSecurityWeakTokenRandom from "./services/be-security.weak-token-random";
import * as fullstackLocalhostEgressCommitted from "./services/fullstack.localhost-egress-committed";
import * as fullstackWsNoAuth from "./services/fullstack.ws-no-auth";
import * as perfApiInLoop from "./services/perf.api-in-loop";
import * as securityTaintFlow from "./services/security.taint-flow";
import * as sqlAppSideAggregationFilterLength from "./services/sql.app-side-aggregation-filter-length";
import * as sqlAppSideAggregationReduce from "./services/sql.app-side-aggregation-reduce";
import * as sqlQueryLogicDensity from "./services/sql.query-logic-density";
import * as srcQueries from "./src/queries";

export const registry = {
  beReliabilityFetchNoTimeout,
  httpDevPathNoGuardHint,
  httpGetRouteNoCacheMarker,
  httpProtectedPathNoAuthEvidence,
  sqlCountInLoop,
  sqlNplus1,
  sqlRaceConditionToctou,
  beDbClientPerRequest,
  beDbEmptyCatchOnWrite,
  beDbExternalCallInTx,
  beDbFloatMoneyCompare,
  beDbPaginationNoOrderby,
  beDbUnawaitedWrite,
  beDbUnboundedUserLimit,
  beDbUpdateDeleteNoWhere,
  beReliabilityAsyncRouteNoCatch,
  beReliabilityAwaitInMap,
  beReliabilityBodyLimitMissing,
  beReliabilityConsoleInBe,
  beReliabilityDebugTrueCommitted,
  beReliabilityEnvNonnullAssert,
  beReliabilityEnvOutsideConfig,
  beReliabilityIntervalNoClear,
  beReliabilityJsonParseNoTry,
  beReliabilityProcessExitInLib,
  beReliabilityPromiseAllWrites,
  beReliabilitySyncFsInHandler,
  beSecurityApiKeyInUrl,
  beSecurityCorsCredentialsWildcard,
  beSecurityCorsWildcard,
  beSecurityErrorLeakToClient,
  beSecurityHardcodedSecret,
  beSecurityInsecureCookie,
  beSecurityJwtNoExpiry,
  beSecurityMassAssignment,
  beSecurityOpenRedirect,
  beSecurityPathTraversal,
  beSecurityRawQueryInterpolation,
  beSecuritySsrfUserUrl,
  beSecurityTimingUnsafeCompare,
  beSecurityWeakPasswordHash,
  beSecurityWeakTokenRandom,
  fullstackLocalhostEgressCommitted,
  fullstackWsNoAuth,
  perfApiInLoop,
  securityTaintFlow,
  sqlAppSideAggregationFilterLength,
  sqlAppSideAggregationReduce,
  sqlQueryLogicDensity,
  srcQueries,
};
