// DECOY — no rule may fire anywhere in this file (a finding here is a false positive by definition).
// Near-misses for the console-write family, each a boundary the C# producer's module doc names: an
// ILogger call inside a loop (configured output with levels and sinks — never a console write),
// Console.ReadLine (not a write), and a bare using-static WriteLine (the site does not spell a chain
// naming Console — the producer claims spellings, not bindings).
using static System.Console;

class ConsoleWriteDecoyCs {
  void LogInLoop(Microsoft.Extensions.Logging.ILogger log, string[] rows) {
    foreach (var row in rows) {
      Microsoft.Extensions.Logging.LoggerExtensions.LogInformation(log, "row {Row}", row);
    }
  }

  void NotAWrite() {
    var line = ReadLine();
    WriteLine(line);
  }
}
