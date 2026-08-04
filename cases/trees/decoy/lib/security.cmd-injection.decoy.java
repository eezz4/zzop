// DECOY — no rule may fire anywhere in this file (a finding here is a false positive by definition).
// The Java half of the same retired class: every method below concatenates an identifier onto a
// string literal (the rule's lexical trigger) in a method that ALSO carries the bare word `exec` or
// `ProcessBuilder`, which is exactly what the pre-W3 matcher accepted as its exec witness. None of
// them constructs a process: `exec` is a field, a parameter and a user method here, and
// `ProcessBuilder` appears only as a type name in a comment-adjacent declaration position that never
// runs. With the witness taken from the parser's projected call sites instead, all three go silent.
class CmdInjectionDecoy {
  private final String exec = "/usr/bin/true";

  String describe(String arg) {
    return "command: " + arg + " via " + exec;
  }

  String viaParameter(String exec, String arg) {
    return "command: " + arg + " (" + exec + ")";
  }

  String exec(String arg) {
    return "would run: " + arg;
  }
}
