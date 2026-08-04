// DECOY — no rule may fire anywhere in this file (a finding here is a false positive by definition).
// Near-misses for the console-write family, each a boundary the Java producer's module doc names:
// an slf4j-style logger inside a loop (configured output with levels and sinks — never a console
// write), and an aliased PrintStream (the site spells `ps`; the producer reads the spelling at the
// site, never a data flow, so the alias degrades to silence rather than to a guess).
class ConsoleWriteDecoy {
  private final org.slf4j.Logger log = org.slf4j.LoggerFactory.getLogger(ConsoleWriteDecoy.class);

  void logInLoop(java.util.List<String> rows) {
    for (String row : rows) {
      log.info("row {}", row);
    }
  }

  void aliased() {
    java.io.PrintStream ps = System.out;
    ps.println("aliased");
  }
}
