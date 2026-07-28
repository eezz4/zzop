package com.example.svc;

import java.util.List;

public interface UserRepository {

  // be-security/annotation-sql-concat — a JPA @Query built by string concatenation.
  @Query("SELECT u FROM User u WHERE u.name = '" + "admin" + "'")
  List<User> byName();
}
