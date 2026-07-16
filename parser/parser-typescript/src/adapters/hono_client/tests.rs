//! Coverage for `extract_hono_client_consumes`: a class-field receiver, a direct `hc(...)` literal
//! base, bracket segments, non-static bases, and the file-gate no-import case.
use super::extract_hono_client_consumes;
use zzop_core::IoConsume;

fn keys(out: &[IoConsume]) -> Vec<Option<String>> {
    out.iter().map(|c| c.key.clone()).collect()
}

#[test]
fn class_field_receiver_auth_client_shape_end_to_end() {
    let src = r#"
import { hc } from 'hono/client';
type AuthClientType = ReturnType<typeof hc<AuthAppType>>;
export class AuthClient {
  public client: AuthClientType;
  constructor(options: { baseUrl: string }) {
    this.client = hc<AuthAppType>(options.baseUrl);
  }
  public async signOut() {
    await this.client.signout.$post();
  }
  public async signOutAllSessions() {
    await this.client['signout-all'].$post();
  }
  public async getSession() {
    const r = await this.client['session-json'].$get();
  }
}
export const authClient = new AuthClient({ baseUrl: `${NEXT_PUBLIC_WEBAPP_URL()}/api/auth` });
"#;
    let out = extract_hono_client_consumes("client/index.ts", src);
    assert_eq!(
        keys(&out),
        vec![
            Some("POST /api/auth/signout".to_string()),
            Some("POST /api/auth/signout-all".to_string()),
            Some("GET /api/auth/session-json".to_string()),
        ]
    );
    assert!(out
        .iter()
        .all(|c| c.kind == "http" && c.raw.is_none() && c.method.is_none()));
    assert_eq!(out[0].line, 10);
    assert_eq!(out[1].line, 13);
    assert_eq!(out[2].line, 16);
}

#[test]
fn direct_string_literal_base_with_dotted_chain() {
    let out = extract_hono_client_consumes(
        "a.ts",
        "import { hc } from 'hono/client';\nconst client = hc<T>('/api/auth');\nclient.two.factor.$post();",
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("POST /api/auth/two/factor"));
    assert_eq!(out[0].line, 3);
}

#[test]
fn bracket_segment_is_a_literal_path_segment() {
    let out = extract_hono_client_consumes(
        "b.ts",
        "import { hc } from 'hono/client';\nconst client = hc('/api/auth');\nclient['signout-all'].$post();",
    );
    assert_eq!(out[0].key.as_deref(), Some("POST /api/auth/signout-all"));
}

#[test]
fn non_static_base_with_no_same_file_trace_is_unresolved() {
    let out = extract_hono_client_consumes(
        "c.ts",
        "import { hc } from 'hono/client';\nconst client = hc(someVar);\nclient.two.factor.$post();",
    );
    assert_eq!(out.len(), 1);
    assert!(out[0].key.is_none());
    assert_eq!(out[0].raw.as_deref(), Some("client.two.factor $post"));
    assert_eq!(out[0].method.as_deref(), Some("POST"));
}

#[test]
fn template_base_with_interpolation_inside_the_path_is_unresolved() {
    let out = extract_hono_client_consumes(
        "d.ts",
        "import { hc } from 'hono/client';\nconst client = hc(`/api/${v}/auth`);\nclient.two.$get();",
    );
    assert_eq!(out.len(), 1);
    assert!(out[0].key.is_none());
    assert_eq!(out[0].raw.as_deref(), Some("client.two $get"));
    assert_eq!(out[0].method.as_deref(), Some("GET"));
}

#[test]
fn no_hono_client_import_yields_nothing_even_with_dollar_verb_calls() {
    let out = extract_hono_client_consumes(
        "e.ts",
        "const client = hc('/api/auth');\nclient.two.factor.$get();",
    );
    assert!(out.is_empty());
}

#[test]
fn bare_param_query_helper_calls_are_skipped_not_path_segments() {
    let out = extract_hono_client_consumes(
        "f.ts",
        "import { hc } from 'hono/client';\nconst client = hc('/api/posts');\nclient[':id'].param({ id: '123' }).$get();",
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /api/posts/{}"));
}
