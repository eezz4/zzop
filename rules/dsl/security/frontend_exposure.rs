use crate::{assert_disqualifier_summary_precedes_imperative, hits, scan, TempDir};

// --- secret-env-in-fe ---

#[test]
fn server_only_secret_env_var_referenced_in_a_tsx_file_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/components/ApiKeyBanner.tsx",
        "export const key = process.env.SUPABASE_SERVICE_ROLE_KEY;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "secret-env-in-fe");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

/// §27 pin (2026-08-29). The disqualifier here is the harshest kind: a prescription the reader can
/// follow EXACTLY and still see red. The message offers the `NEXT_PUBLIC_`/`VITE_` rename for a value
/// that was never secret, and then -- one sentence too late -- says the prefix does not silence the
/// finding, because the name test reads `SERVICE_ROLE` ANYWHERE in the variable name and
/// `NEXT_PUBLIC_SUPABASE_SERVICE_ROLE_KEY` matches exactly as the unprefixed spelling does.
///
/// WHAT MOVED: two adjacent sentences swapped, nothing else. 4509 characters before and after,
/// character multiset identical. The clause could not go any further forward than this: it opens "AND
/// THE PREFIX", whose antecedent is the prefix named in the sentence now directly ahead of it, and
/// lifting it over that sentence would have cost an antecedent repair -- which is an edit, not a move.
///
/// The imperative pinned is "Only reach for that prefix", the message's first POSITIVE instruction. The
/// clause does still sit behind one earlier verb, "do not just rename it with a ... prefix", and that is
/// harmless by inspection rather than by luck: that verb PROHIBITS the very action the clause is about,
/// so a reader who obeys it cannot be the reader the clause exists for. The one who can is the reader
/// granted permission by the next sentence, and that permission now arrives second.
///
/// INVALIDATION PROBE: swap the two sentences back. Both stay spelled exactly once, a `contains` pin
/// stays green, and this assertion alone goes red.
#[test]
fn secret_env_in_fe_prefix_disqualifier_precedes_the_permission_to_use_it() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/components/ApiKeyBanner.tsx",
        "export const key = process.env.SUPABASE_SERVICE_ROLE_KEY;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "secret-env-in-fe");
    // Sentence order only -- the finding itself is unchanged.
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
    assert_disqualifier_summary_precedes_imperative(
        "secret-env-in-fe",
        &h[0].message,
        "AND THE PREFIX DOES NOT SILENCE THIS FINDING",
        "Only reach for that prefix",
        "a prescription you can follow exactly and still see red",
    );
}

#[test]
fn public_env_var_referenced_in_a_tsx_file_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "src/components/PublicConfig.tsx",
        "export const apiUrl = process.env.NEXT_PUBLIC_API_URL;\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "secret-env-in-fe").is_empty(),
        "{:?}",
        out.findings
    );
} // --- secret-env-in-fe: Next.js server-only path shapes ---
  //
  // The FRAMEWORK's own routing convention is the declaration these three exemptions stand on (§24: a
  // rule that ERASES findings must read a declaration in the scanned source, never an inference). Next.js
  // fixes all three spellings, so none of them is a name the project chose:
  //   * `app/**/route.<js-ts>` — an App Router ROUTE HANDLER. The filename `route` under `app/` IS the
  //     declaration; the `api/` segment cal.com happens to use is decoration Next does not require, so the
  //     arm keys on the filename rather than on the directory (§5.6: exempt the shape of the justifying
  //     case, not a broader property it happens to have — "any path containing `api`" is that broader
  //     property, and it would silence real client code under a directory someone named `api`).
  //   * `pages/api/**` — a Pages Router API route. Here the DIRECTORY is Next's declaration: it excludes
  //     the whole subtree from the client bundle. Restricted to non-SFC extensions so a Nuxt/SvelteKit
  //     tree, where `pages/` holds `.vue` PAGES, cannot be swept in by a directory literally named `api`.
  //   * `next.config.<js-ts>` — Next's server config, read by the Node build process.
  //
  // Measured on cal.com (2026-08-25): the rule fired 6 times, every one of them inside `apps/web/` and for
  // no other reason than the Turborepo/Next convention of naming the app directory `web`. Four were App
  // Router route handlers, one was `next.config.ts`. The sixth (`apps/web/cron-tester.ts`, a dotenv Node
  // script) DELIBERATELY still fires: nothing in its path is a framework declaration, and inferring
  // "top-level .ts + dotenv = a script" is exactly the inference §24 forbids in the erasing direction.

#[test]
fn next_app_router_route_handler_reading_a_server_secret_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "apps/web/app/api/cron/route.ts",
        "export async function GET() {\n  return new Response(process.env.CRON_SECRET);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "secret-env-in-fe").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn next_app_router_route_handler_outside_an_api_segment_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "apps/web/app/webhooks/stripe/route.ts",
        "export async function POST() {\n  return new Response(process.env.STRIPE_PRIVATE_KEY);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "secret-env-in-fe").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn next_pages_router_api_route_reading_a_server_secret_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "apps/web/pages/api/managed-user.ts",
        "export default async function handler() {\n  return process.env.X_CAL_SECRET_KEY;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "secret-env-in-fe").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn next_server_config_reading_a_server_secret_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "apps/web/next.config.ts",
        "if (!process.env.NEXTAUTH_SECRET) throw new Error(\"set NEXTAUTH_SECRET\");\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "secret-env-in-fe").is_empty(),
        "{:?}",
        out.findings
    );
}

// The three canaries below are the OTHER half of §5.10's two-way instrument check: each is a shape the
// exemption must NOT reach, and each fires today and must keep firing.

#[test]
fn a_client_component_under_the_same_web_app_directory_is_still_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "apps/web/components/ApiKeyBanner.tsx",
        "\"use client\";\nexport const key = process.env.SUPABASE_SERVICE_ROLE_KEY;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "secret-env-in-fe");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn a_helper_module_beside_a_route_handler_is_still_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "apps/web/app/api/cron/helpers.ts",
        "export const key = process.env.CRON_SECRET;\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "secret-env-in-fe");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn an_app_router_page_component_is_still_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "apps/web/app/dashboard/page.tsx",
        "export default function Page() {\n  return <div>{process.env.SUPABASE_SERVICE_ROLE_KEY}</div>;\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "secret-env-in-fe");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn a_nuxt_style_vue_page_under_a_pages_api_directory_is_still_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "pages/api/Explorer.vue",
        "<script setup lang=\"ts\">\nconst k = import.meta.env.VITE_PRIVATE_TOKEN;\n</script>\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "secret-env-in-fe");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

// --- localstorage-jwt ---

#[test]
fn token_written_to_local_storage_is_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "web/auth.ts",
        "export function saveToken(token: string) {\n  localStorage.setItem(\"token\", token);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "localstorage-jwt");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn non_token_value_written_to_local_storage_is_not_flagged() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "web/prefs.ts",
        "export function saveTheme(theme: string) {\n  localStorage.setItem(\"theme\", theme);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "localstorage-jwt").is_empty(),
        "{:?}",
        out.findings
    );
}
