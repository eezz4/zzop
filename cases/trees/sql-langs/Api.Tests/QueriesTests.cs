// TEST TWIN of Api/Queries.cs — the C# convention, exercising both arms the shared test-path vocabulary
// gained on 2026-08-10 at once: the `*Tests.cs` FILE suffix and the `My.Tests/` PROJECT DIRECTORY. Every
// finding the production twin carries is expected to be ABSENT here; any finding at all scores as a false
// positive (`benign` in EXPECTED.jsonc).
public class QueriesTests
{
    public void Queries()
    {
        var a = "DELETE FROM sqlx_cs_sessions";
        var b = "UPDATE sqlx_cs_accounts SET balance = 0";
        var c = "TRUNCATE TABLE sqlx_cs_audit_log";
        var d = "SELECT * FROM sqlx_cs_users";
        var e = "SELECT id FROM sqlx_cs_users WHERE name LIKE '%term'";
    }
}
