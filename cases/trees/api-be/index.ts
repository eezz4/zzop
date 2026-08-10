// Entry barrel (generated; keep in sync with api/ + services/ + src/). Namespace-imports every rule
// module — a module missing from here is an unimported orphan and picks up `dead-candidates` +
// `unimported-export` on top of whatever it was planted to test (measured, not assumed: the two nested
// `sql/nplus1` fixtures did exactly that before they were added below).
import * as beReliabilityFetchNoTimeout from "./api/be-reliability.fetch-no-timeout";
import * as crossLayerSensitiveResponseField from "./api/cross-layer.sensitive-response-field";
import * as httpDevPathNoGuardHint from "./api/http.dev-path-no-guard-hint";
import * as httpProtectedPathNoAuthEvidence from "./api/http.protected-path-no-auth-evidence";
import * as sqlCountInLoop from "./api/sql.count-in-loop";
import * as sqlNplus1 from "./api/sql.nplus1";
import * as sqlRaceConditionToctou from "./api/sql.race-condition-toctou";
import * as routesSqlRaceConditionToctouToproutes from "./routes/sql.race-condition-toctou-toproutes";
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
import * as beReliabilityConsoleInLoop from "./services/be-reliability.console-in-loop";
import * as beReliabilityDebugTrueCommitted from "./services/be-reliability.debug-true-committed";
import * as beReliabilityEnvNonnullAssert from "./services/be-reliability.env-nonnull-assert";
import * as beReliabilityEnvOutsideConfig from "./services/be-reliability.env-outside-config";
import * as beReliabilityIntervalNoClear from "./services/be-reliability.interval-no-clear";
import * as beReliabilityJsonParseNoTry from "./services/be-reliability.json-parse-no-try";
import * as beReliabilityProcessExitInLib from "./services/be-reliability.process-exit-in-lib";
import * as beReliabilityPromiseAllWrites from "./services/be-reliability.promise-all-writes";
import * as beReliabilitySyncFsInHandler from "./services/be-reliability.sync-fs-in-handler";
import * as beSecurityApiKeyInUrl from "./services/be-security.api-key-in-url";
import * as beSecurityBcryptCostTooLow from "./services/be-security.bcrypt-cost-too-low";
import * as beSecurityCorsCredentialsWildcard from "./services/be-security.cors-credentials-wildcard";
import * as beSecurityCorsWildcard from "./services/be-security.cors-wildcard";
import * as beSecurityErrorLeakToClient from "./services/be-security.error-leak-to-client";
import * as beSecurityHardcodedSecret from "./services/be-security.hardcoded-secret";
import * as beSecurityHighEntropySecret from "./services/be-security.high-entropy-secret";
import * as beSecurityInsecureCookie from "./services/be-security.insecure-cookie";
import * as beSecurityJwtNoExpiry from "./services/be-security.jwt-no-expiry";
import * as beSecurityMassAssignment from "./services/be-security.mass-assignment";
import * as beSecurityOpenRedirect from "./services/be-security.open-redirect";
import * as beSecurityPathTraversal from "./services/be-security.path-traversal";
import * as beSecurityRawQueryInterpolation from "./services/be-security.raw-query-interpolation";
import * as beSecurityShellExecInterpolation from "./services/be-security.shell-exec-interpolation";
import * as beSecuritySsrfUserUrl from "./services/be-security.ssrf-user-url";
import * as beSecurityTimingUnsafeCompare from "./services/be-security.timing-unsafe-compare";
import * as beSecurityWeakCrypto from "./services/be-security.weak-crypto";
import * as beSecurityWeakPasswordHash from "./services/be-security.weak-password-hash";
import * as beSecurityWeakTokenRandom from "./services/be-security.weak-token-random";
import * as fullstackLocalhostEgressCommitted from "./services/fullstack.localhost-egress-committed";
import * as fullstackWsNoAuth from "./services/fullstack.ws-no-auth";
import * as perfApiInLoop from "./services/perf.api-in-loop";
import * as securityTaintFlow from "./services/security.taint-flow";
import * as sqlAppSideAggregationFilterLength from "./services/sql.app-side-aggregation-filter-length";
import * as sqlAppSideAggregationReduce from "./services/sql.app-side-aggregation-reduce";
import * as sqlQueryLogicDensity from "./services/sql.query-logic-density";
import * as spansDbEmptyCatchAndWrite from "./spans/db.empty-catch-and-write.span";
import * as spansDbMultiWriteNoTx from "./spans/db.multi-write-no-tx.span";
import * as spansDbUpdateDeleteNoWhere from "./spans/db.update-delete-no-where.span";
import * as spansSecurityHtmlResponseFromRequest from "./spans/security.html-response-from-request.span";
import * as spansSecurityInsecureCookie from "./spans/security.insecure-cookie.span";
import * as spansSecurityMassAssignment from "./spans/security.mass-assignment.span";
import * as spansSecurityOpenRedirect from "./spans/security.open-redirect.span";
import * as spansSecurityPathTraversal from "./spans/security.path-traversal.span";
import * as spansSecuritySsrfUserUrl from "./spans/security.ssrf-user-url.span";
import * as spansSecurityTaintFlow from "./spans/security.taint-flow.span";
import * as srcApiSqlNplus1Nested from "./src/api/sql.nplus1-nested";
import * as srcDomainsOrdersRoutesSqlNplus1Domain from "./src/domains/orders/routes/sql.nplus1-domain";
import * as srcQueries from "./src/queries";

export const registry = {
  beReliabilityFetchNoTimeout,
  crossLayerSensitiveResponseField,
  httpDevPathNoGuardHint,
  httpProtectedPathNoAuthEvidence,
  sqlCountInLoop,
  sqlNplus1,
  sqlRaceConditionToctou,
  routesSqlRaceConditionToctouToproutes,
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
  beReliabilityConsoleInLoop,
  beReliabilityDebugTrueCommitted,
  beReliabilityEnvNonnullAssert,
  beReliabilityEnvOutsideConfig,
  beReliabilityIntervalNoClear,
  beReliabilityJsonParseNoTry,
  beReliabilityProcessExitInLib,
  beReliabilityPromiseAllWrites,
  beReliabilitySyncFsInHandler,
  beSecurityApiKeyInUrl,
  beSecurityBcryptCostTooLow,
  beSecurityCorsCredentialsWildcard,
  beSecurityCorsWildcard,
  beSecurityErrorLeakToClient,
  beSecurityHardcodedSecret,
  beSecurityHighEntropySecret,
  beSecurityInsecureCookie,
  beSecurityJwtNoExpiry,
  beSecurityMassAssignment,
  beSecurityOpenRedirect,
  beSecurityPathTraversal,
  beSecurityRawQueryInterpolation,
  beSecurityShellExecInterpolation,
  beSecuritySsrfUserUrl,
  beSecurityTimingUnsafeCompare,
  beSecurityWeakCrypto,
  beSecurityWeakPasswordHash,
  beSecurityWeakTokenRandom,
  fullstackLocalhostEgressCommitted,
  fullstackWsNoAuth,
  perfApiInLoop,
  securityTaintFlow,
  sqlAppSideAggregationFilterLength,
  sqlAppSideAggregationReduce,
  sqlQueryLogicDensity,
  spansDbEmptyCatchAndWrite,
  spansDbMultiWriteNoTx,
  spansDbUpdateDeleteNoWhere,
  spansSecurityHtmlResponseFromRequest,
  spansSecurityInsecureCookie,
  spansSecurityMassAssignment,
  spansSecurityOpenRedirect,
  spansSecurityPathTraversal,
  spansSecuritySsrfUserUrl,
  spansSecurityTaintFlow,
  srcApiSqlNplus1Nested,
  srcDomainsOrdersRoutesSqlNplus1Domain,
  srcQueries,
};
