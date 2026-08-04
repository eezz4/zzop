// be-reliability/console-in-be (C# lane) — bad: Console writes in backend-path source, in both the
// direct spelling and the System.-prefixed Error-writer spelling (the callee carries the whole chain
// as written). good: ILogger — configured output with levels and sinks, which the call-site channel
// deliberately never folds into console-write.
class BeReliabilityConsoleInBe {
  void Bad() {
    Console.WriteLine("processing request");
  }

  void BadStderr() {
    System.Console.Error.WriteLine("request failed");
  }

  void Good(Microsoft.Extensions.Logging.ILogger log) {
    Microsoft.Extensions.Logging.LoggerExtensions.LogInformation(log, "processing request");
  }
}
