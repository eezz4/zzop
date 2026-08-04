// cross-layer/sensitive-response-field — bad: routes whose DECLARED return type (response-shape-v1:
// the return-type annotation, Promise<X> unwrapped, resolved against the tree-wide class/interface
// shape merge) carries a sensitive-NAMED field. Provide-side only: these routes need no consumer to
// fire (the declaration is the evidence, so severity stays warning here); the consumed->CRITICAL
// escalation half lives in the xlayer pair (xbe/session.controller.ts + xfe/consumesSession.ts).
// Negative controls: a clean declared shape, and an undeclared handler — which must stay SILENT
// (never guessed from the body) and is disclosed on this tree's warnings instead.
declare function Controller(prefix: string): ClassDecorator;
declare function Get(path?: string): MethodDecorator;

class AccountSummaryDto {
  id: string;
  email: string;
  passwordHash: string; // substring axis: normalized name contains `password`
}

class SessionViewDto {
  id: string;
  refreshToken: string; // suffix axis: normalized name ends with `token` (`tokenizer` would not)
}

interface PublicProfileDto {
  id: string;
  displayName?: string;
}

@Controller('account-shapes')
export class AccountShapesController {
  // bad: declared response carries passwordHash
  @Get('summary')
  getSummary(): Promise<AccountSummaryDto> {
    return Promise.resolve(new AccountSummaryDto());
  }

  // bad: declared response carries refreshToken (suffix axis)
  @Get('session-view')
  getSessionView(): Promise<SessionViewDto> {
    return Promise.resolve(new SessionViewDto());
  }

  // good: clean declared shape (interface referent — pins interface resolution too)
  @Get('public-profile')
  getPublicProfile(): Promise<PublicProfileDto> {
    return Promise.resolve({ id: 'p1', displayName: 'n' });
  }

  // good (undeclared): returns the same sensitive data at runtime, but declares NO return type —
  // no fact, no finding, and the tree's warnings say "declare a return type" instead.
  @Get('legacy-summary')
  getLegacySummary() {
    return Promise.resolve(new AccountSummaryDto());
  }
}
