package com.example.svc.services;

// Console-hygiene fixtures for the Java lane of the call-site channel. bad: System.out/System.err
// writes in backend-path source (this file sits under a services/ segment), one of them proven by the
// parser's loop span to run once per iteration. good: an slf4j-style logger — configured output with
// levels and sinks, which the call-site channel deliberately never folds into console-write.
class ConsoleHygiene {
  void badPlain() {
    System.out.println("processing request");
  }

  void badLoop(java.util.List<String> orderIds) {
    for (String id : orderIds) {
      System.err.println("processing order " + id);
    }
  }

  void goodLogger(org.slf4j.Logger log) {
    log.info("processing request");
  }
}
