// PRODUCTION TWIN of Api.Tests/UserTests.cs — the same defects on a path that is not a test path. Its
// findings are expectations in EXPECTED.jsonc; the twin's silence is a `benign` entry. See the Go pair's
// header for why neither half proves anything alone.
using System;
using System.Security.Cryptography;
using System.Text;

public class UserService
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
