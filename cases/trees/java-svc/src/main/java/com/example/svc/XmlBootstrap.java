package com.example.svc;

import java.io.InputStream;
import javax.xml.parsers.DocumentBuilderFactory;

// security/xxe-no-guard -- an XML factory built in a `static { }` initializer with NO hardening.
//
// This fixture exists for the SPAN, not for the regex. The class also holds an ordinary method, so
// `dsl::method_scan::gates::drop_outer_spans` discards the class-wide span in favour of that method's
// leaf -- and until static initializers projected a leaf of their own (2026-08-10) the block below sat
// in no span at all, so this critical finding was structurally unreachable. The mechanism was
// identified by deleting `parse` below: with no sibling leaf the class span survived, the same
// `newInstance()` line became visible, and the finding appeared. Keep both members.
public class XmlBootstrap {

  private static final DocumentBuilderFactory FACTORY;

  static {
    FACTORY = DocumentBuilderFactory.newInstance();
  }

  public void parse(InputStream in) throws Exception {
    FACTORY.newDocumentBuilder().parse(in);
  }
}
