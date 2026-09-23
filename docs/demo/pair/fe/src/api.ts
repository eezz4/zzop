// The FRONTEND half of the break-a-route demo. See ../../../break-a-route.md.
//
// Four calls, written the ordinary way. This tree has no dependency on the backend tree: no import, no
// generated client, no shared package. Each path is a plain string, and one of them is a template
// literal with an interpolated id -- which the backend spells `:id`, in a route that does not carry
// the `/api` prefix either. Neither side's text matches the other's.
export async function loadProfile() {
  return fetch('/api/profile');
}

export async function saveProfile(body: unknown) {
  return fetch('/api/profile', { method: 'PUT', body: JSON.stringify(body) });
}

export async function loadUser(id: string) {
  return fetch(`/api/users/${id}`);
}

export async function signIn(body: unknown) {
  return fetch('/api/session', { method: 'POST', body: JSON.stringify(body) });
}
