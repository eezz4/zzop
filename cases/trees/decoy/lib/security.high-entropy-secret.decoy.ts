// DECOY for security/high-entropy-secret. In scope, provably: `.ts` matches file_pattern, the binding
// name ends in `token` (name_pattern), no mock-family word is anywhere in the name, and the value
// measures 105.4 total Shannon bits — ABOVE the measured 80-bit floor, so neither the name gate nor
// the entropy gate stands between this line and a finding. Only `skip_value_equals_name` does: the
// value is LITERALLY its own binding name (the sentinel/error-code idiom), vetoed by hash equality
// (`value_hash == value_hash_hex(name)`) through the real producer -> rule path — this is the one
// corpus probe of that veto above the floor, where the veto alone decides.
// (`hardcoded-secret` stays silent on its own: the value is identifier-shaped, its veto #4's business.)
export const external_partner_gateway_token = 'external_partner_gateway_token';
