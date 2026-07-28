package com.example.svc;

import java.sql.*;
import java.io.*;
import java.util.Random;
import java.util.List;
import javax.xml.parsers.DocumentBuilderFactory;
import java.security.MessageDigest;
import javax.net.ssl.HostnameVerifier;
import javax.servlet.http.HttpServletRequest;
import javax.servlet.http.HttpServletResponse;

// Java detections (bad patterns). Java is lexically parsed, so this need not compile — each method plants
// one rule trigger. Good/correct forms are documented per rule in the plan (Java good examples TBD).
public class UnsafeController {

  // java-security/sql-taint — SQL built by string concatenation.
  public ResultSet find(Connection c, String name) throws Exception {
    String sql = "SELECT * FROM users WHERE name = '" + name + "'";
    return c.createStatement().executeQuery(sql);
  }

  // java-security/weak-crypto — MD5.
  public byte[] hash(String s) throws Exception {
    return MessageDigest.getInstance("MD5").digest(s.getBytes());
  }

  // java-security/cmd-injection — Runtime.exec with concatenation.
  public void run(String arg) throws Exception {
    Runtime.getRuntime().exec("/bin/sh -c " + arg);
  }

  // be-security/java-hardcoded-password — JDBC credentials in source.
  public Connection connect() throws Exception {
    return DriverManager.getConnection("jdbc:mysql://db/app", "admin", "s3cr3t-p@ss");
  }

  // be-security/xxe-no-guard — DocumentBuilderFactory with no XXE hardening.
  public void parse(InputStream in) throws Exception {
    DocumentBuilderFactory f = DocumentBuilderFactory.newInstance();
    f.newDocumentBuilder().parse(in);
  }

  // be-security/unsafe-deserialization — native readObject on a stream.
  public Object load(InputStream in) throws Exception {
    return new ObjectInputStream(in).readObject();
  }

  // be-security/java-path-traversal — request value joined into a File path.
  public File open(HttpServletRequest request) {
    return new File("/srv/" + request.getParameter("name"));
  }

  // be-security/java-weak-random — new Random() for a token.
  public String token() {
    String token = "s-" + new Random().nextInt();
    return token;
  }

  // be-security/stacktrace-to-response — stack trace / message reaches the HTTP response.
  public void handle(Exception e, HttpServletResponse response) throws Exception {
    e.printStackTrace();
    response.getWriter().write(e.getMessage());
  }

  // be-security/trust-all-tls — an always-true hostname verifier installed on a connection.
  public void disableTls(javax.net.ssl.HttpsURLConnection conn) {
    conn.setHostnameVerifier((h, s) -> true);
  }
}
