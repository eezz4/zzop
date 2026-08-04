// be-reliability/console-in-loop (C# lane) — bad: a Console write the parser places INSIDE a foreach
// statement's projected span, so it runs once per iteration. good: one aggregated write after the
// loop — that line still co-fires console-in-be (this file sits under services/), a different claim;
// same pairing as the TS and Go fixtures.
class BeReliabilityConsoleInLoop {
  void Bad(string[] orderIds) {
    foreach (var id in orderIds) {
      Console.WriteLine("processing order " + id);
    }
  }

  void Good(string[] orderIds) {
    foreach (var id in orderIds) {
      Accumulate(id);
    }
    Console.WriteLine(orderIds.Length);
  }

  void Accumulate(string id) {}
}
