-- 0002: retire the legacy table. A REAL .sql migration -- the sql pack had never been
-- scored on one (review ledger V147).
DROP TABLE legacy_orders;
TRUNCATE audit_log;
