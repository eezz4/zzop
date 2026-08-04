// cross-layer/sensitive-response-field ESCALATION half — this declared-sensitive route IS consumed
// (xfe/consumesSession.ts fetches /session/report), so the finding fires at CRITICAL with the
// consumer count. The unconsumed warning-severity half lives in
// api/api/cross-layer.sensitive-response-field.ts.
declare function Controller(prefix: string): ClassDecorator;
declare function Get(path?: string): MethodDecorator;

class SessionReportDto {
  id: string;
  sessionToken: string; // suffix axis: normalized name ends with `token`
}

@Controller('session')
export class SessionController {
  @Get('report')
  getReport(): Promise<SessionReportDto> {
    return Promise.resolve(new SessionReportDto());
  }
}
