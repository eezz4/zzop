// The front end's own config module. `baseApiUrl` is an ordinary cross-file constant — the shape a real
// front end uses, and the shape services/client.ts feeds to `axios.defaults.baseURL`.
export const settings = {
  baseApiUrl: '/api',
};
