// TEST TWIN of Api/UserService.cs — the C# convention, and the only fixture here that exercises TWO of
// the arms the 2026-08-10 merge added at once: the `*Tests.cs` FILE suffix and the `My.Tests/` PROJECT
// DIRECTORY. Neither was in the DSL's `${test-paths}` vocabulary before that date.
//
// Every finding the production twin carries is expected to be ABSENT here; any finding at all scores as
// a false positive (`benign` in EXPECTED.jsonc). The rules this pair exercises are gated on the `.cs`
// extension alone, not on a backend path segment — which is what lets the file sit at the idiomatic
// `Api.Tests/` location instead of somewhere contrived.
using System;
using System.Security.Cryptography;
using System.Text;

public class UserTests
{
    public void Audit(int[] userIds)
    {
        foreach (var uid in userIds)
        {
            Console.WriteLine($"checking {uid}");
        }
    }

    public byte[] Digest(string password)
    {
        using var md5 = MD5.Create();
        return md5.ComputeHash(Encoding.UTF8.GetBytes(password));
    }
}
