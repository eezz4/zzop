// http/protected-path-no-auth-evidence — the DECORATOR half of the same rule, added 2026-09-05. The
// sibling fixture beside this one covers the only clearing mechanism this rule had until now: an
// `auth-guarded` attribute injected by an adapter overlay. This one covers the other producer of that
// same attribute — `decorator_guarded`, which the engine mints into `auth-guarded` for any route line a
// NestJS class/method decorator guards (crates/engine/.../rules/io_scan.rs `mint_auth_guarded`).
//
// What it pins is a VOCABULARY, and the three bad controllers are the point of the file: a decorator
// that carries auth words is not the same thing as a decorator that gates. Measured 2026-09-05 across
// three real projects, this rule was wrong on 4 of 4 first-screen rows, and a house decorator whose own
// name says it authenticates was one of the shapes it could not see.
declare function Controller(prefix: string): ClassDecorator;
declare function Get(path?: string): MethodDecorator;
declare function Authenticated(): ClassDecorator;
declare function ApiBearerAuth(): ClassDecorator;
declare function SkipAuth(): ClassDecorator;
declare function Roles(role: string): ClassDecorator;

// NEGATIVE CONTROL — a HOUSE decorator whose own name says it authenticates. This is what a NestJS
// codebase writes once it has wrapped its guard a single time, and it clears the route exactly as
// `@UseGuards` would.
@Controller('admin/reports')
@Authenticated()
export class AdminReportsController {
  @Get('daily')
  daily() {}
}

// BAD 1 — `@ApiBearerAuth` is `@nestjs/swagger`. It writes a line in an OpenAPI document and enforces
// nothing at all, so the route is as open as one with no decorator.
@Controller('admin/exports')
@ApiBearerAuth()
export class AdminExportsController {
  @Get('daily')
  daily() {}
}

// BAD 2 — `@SkipAuth` carries the same word and means the opposite. Reading it as evidence would go
// silent on exactly the route a reader most needs told about. (The path avoids `debug`/`dev` on purpose:
// the sibling rule `http/dev-path-no-guard-hint` matches those words and this file pins one rule.)
@Controller('admin/purge')
@SkipAuth()
export class AdminPurgeController {
  @Get('dump')
  dump() {}
}

// BAD 3 — `@Roles` is policy METADATA a guard consumes, not a guard. NestJS puts it BESIDE
// `@UseGuards(RolesGuard)`; standing alone it enforces nothing, which is the shape this rule exists for.
@Controller('admin/roles')
@Roles('admin')
export class AdminRolesController {
  @Get('list')
  list() {}
}
