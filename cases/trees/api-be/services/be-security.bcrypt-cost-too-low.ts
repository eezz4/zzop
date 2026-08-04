// be-security/bcrypt-cost-too-low — bad: a single-digit bcrypt cost factor. good: a two-digit one.
//
// Split out of weak-password-hash on 2026-08-03 when that rule became structural: bcrypt constructs no
// digest the parser can witness (it is the recommended answer, not the defect), so the cost check
// stays lexical and lives under its own id. The sibling fixture beside this one is the structural
// half, and neither file fires the other's rule — which is the point of the split.
import bcrypt from 'bcrypt';

export function bad(password: string) {
  return bcrypt.hashSync(password, 4);
}

export function good(password: string) {
  return bcrypt.hashSync(password, 12);
}
