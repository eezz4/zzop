package com.example.svc;

import java.io.File;

// The JAVA arm of the method-scan span-boundary axis (the TypeScript arm, and the axis itself, are
// described in trees/api-be/spans/README.md).
//
// Java does not have the TypeScript shape: this parser emits a span for every method
// (parser/parser-java-21/src/lang/symbols/member.rs), so a class span is always dropped in favour of
// its methods and a property-only class is not a thing. What Java has instead is the LAMBDA:
// "a lambda body simply falls within its enclosing METHOD's own body_start..=body_end span"
// (parser/parser-java-21/src/lang/calls.rs). A route-registration method holding several handler
// lambdas is therefore one span containing several independent functions, and a `method-scan` rule
// pairs its patterns freely across them.
//
// HONEST CAVEAT, because the corpus is not allowed to overstate what it proves. The three Java rules
// on this axis all word their claim as "in the same METHOD", and these lambdas are, textually, in the
// same method — so a firing here is a weaker finding than its TypeScript sibling, where the rule says
// "in the same function" and the span is a whole class. What it does show is that the axis is not
// TypeScript-only, which matters for how wide a projection repair has to reach.
//
// Java is lexically parsed here, so this need not compile. The TP CONTROLS for both rules already
// exist and are labeled: UnsafeController.open (java-path-traversal) and UnsafeController.handle
// (stacktrace-to-response). This file adds only the probes.
public class LambdaRoutes {

  private static final String ASSET_ROOT = "/srv/app/assets";

  private AssetIndex index;
  private ReportExporter exporter;
  private RebuildScheduler scheduler;

  // FP PROBE for security/java-path-traversal. The first handler serves one hard-coded file name from
  // a constant root; the second handler reads a query parameter and answers from an in-memory index
  // that never touches the disk. No parameter reaches any path.
  public void registerAssetRoutes(Router router) {
    router.get("/assets/logo", (request, response) -> new File(ASSET_ROOT, "logo.svg"));
    router.get("/assets/search", (request, response) -> index.lookup(request.getParameter("q")));
  }

  // FP PROBE for security/stacktrace-to-response. The first handler writes a CSV body and never sees an
  // exception; the second handler logs a background-rebuild failure to stderr and answers with a fixed
  // string. The trace and the response writer are in two different handlers, and the trace reaches
  // neither the client nor the other handler.
  public void registerReportRoutes(Router router) {
    router.get("/reports/export", (request, response) -> {
      response.type("text/csv");
      exporter.writeCsv(response.raw().getWriter());
      return "";
    });
    router.post("/reports/rebuild", (request, response) -> {
      try {
        scheduler.rebuild();
      } catch (Exception e) {
        e.printStackTrace();
      }
      return "queued";
    });
  }
}
