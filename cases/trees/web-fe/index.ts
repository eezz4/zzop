// Entry barrel. Namespace-imports every rule module (except dead.orphan) so each file has fan-in (not a
// dead-candidate) and its exports are consumed (not dead-exports) — isolating each module's signal to its
// own planted `bad` pattern. dead.orphan is intentionally left out so it stays dead.
import * as asCast from './rules/typescript.as-cast';
import * as noExplicitAny from './rules/typescript.no-explicit-any';
import * as unhandledPromise from './rules/typescript.unhandled-promise-use-effect';
import * as asyncHandler from './rules/typescript.async-handler-no-try';
import * as noSystemDialogs from './rules/browser.no-system-dialogs';
import * as noDocumentWrite from './rules/browser.no-document-write';
import * as mixedContent from './rules/fullstack.mixed-content-egress';
import * as getWithBody from './rules/fullstack.get-with-body';
import * as localstorageJwt from './rules/be-security.localstorage-jwt';
import * as secretEnvInFe from './rules/be-security.secret-env-in-fe';

export const registry = {
  asCast,
  noExplicitAny,
  unhandledPromise,
  asyncHandler,
  noSystemDialogs,
  noDocumentWrite,
  mixedContent,
  getWithBody,
  localstorageJwt,
  secretEnvInFe,
};
