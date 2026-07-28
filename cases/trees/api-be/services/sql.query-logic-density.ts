// sql/query-logic-density — bad: a SQL literal embedding 2+ CASE WHEN branches. good: a plain query.
export const bad =
  'SELECT id, CASE WHEN a THEN 1 ELSE 0 END AS x, CASE WHEN b THEN 2 ELSE 0 END AS y FROM t';

export const good = 'SELECT id, name FROM t WHERE active = true';
