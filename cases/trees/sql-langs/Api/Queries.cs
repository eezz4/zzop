// PRODUCTION TWIN of Api.Tests/QueriesTests.cs — the C# half of the same five literals the Python, Go and
// Java twins in this tree carry. Table names are tree-unique (`sqlx_`) so these literals cannot move
// another tree's expected set through the `db-table` channel.
public class Queries
{
    // sql/delete-no-where — a closed literal holding a whole-table DELETE.
    public const string PurgeSessions = "DELETE FROM sqlx_cs_sessions";

    public const string GoodPurgeSessions = "DELETE FROM sqlx_cs_sessions WHERE expires_at < now()";

    // sql/update-no-where — the same discipline, for UPDATE.
    public const string ResetBalances = "UPDATE sqlx_cs_accounts SET balance = 0";

    public const string GoodResetBalances = "UPDATE sqlx_cs_accounts SET balance = 0 WHERE closed_at IS NOT NULL";

    // sql/truncate-in-app-code — TRUNCATE outside a migration directory.
    public const string WipeAuditLog = "TRUNCATE TABLE sqlx_cs_audit_log";

    public const string GoodWipeAuditLog = "DELETE FROM sqlx_cs_audit_log WHERE created_at < now() - interval '90 days'";

    // sql/select-star — `SELECT *` inside a literal.
    public const string AllUsers = "SELECT * FROM sqlx_cs_users";

    public const string GoodAllUsers = "SELECT id, email FROM sqlx_cs_users";

    // sql/like-leading-wildcard — a leading `%` cannot use a B-tree index prefix.
    public const string SearchUsers = "SELECT id FROM sqlx_cs_users WHERE name LIKE '%term'";

    public const string GoodSearchUsers = "SELECT id FROM sqlx_cs_users WHERE name LIKE 'term%'";

    // --- C# quote-form evidence: which spellings the line-scan can and cannot reach ---

    // A VERBATIM literal puts the `@` BEFORE the quote, so the quote is still the character adjacent to
    // the keyword — the same shape the rule reads, and it FIRES.
    public const string VerbatimDelete = @"DELETE FROM sqlx_cs_jobs";

    // The disclosed residual, planted so it is measured rather than assumed: a verbatim literal spanning
    // lines puts the statement on a line that carries no quote at all. SILENT.
    public const string VerbatimBlock = @"
        DELETE FROM sqlx_cs_events
    ";

    // C# composite formatting uses `{0}`, not `%`, so an interpolated pattern never reaches the `%`
    // anchor in the first place — this line is silent for a different reason than Go's and Python's, and
    // it is here so that difference is recorded rather than assumed.
    public static string LikePlaceholder(string term) => string.Format("SELECT id FROM sqlx_cs_users WHERE name LIKE '{0}'", term);

    // The interpolation half of the destructive rules' never-guess discipline: the table slot cannot be a
    // brace, so an interpolated target is SILENT rather than a guess.
    public static string DeleteFromInterpolated(string table) => $"DELETE FROM {table}";
}
