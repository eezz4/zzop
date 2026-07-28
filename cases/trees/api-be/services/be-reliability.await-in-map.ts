// be-reliability/await-in-map — bad: .map(async …) with no Promise.all (rejections go unhandled). good:
// the mapped promises awaited together with Promise.all.
declare function process_(id: string): Promise<void>;

export function bad(ids: string[]) {
  ids.map(async (id) => {
    await process_(id);
  });
}

export function good(ids: string[]) {
  return Promise.all(ids.map((id) => process_(id)));
}
