// typescript/as-cast — bad: a genuine `as` type cast. good: a JSX polymorphic `as=` prop (not a cast).
export const bad = (raw: unknown) => raw as unknown as string; // force-cast — the narrowed rule only flags `as any` / `as unknown as`

export const good = () => <Box as="span">ok</Box>;
