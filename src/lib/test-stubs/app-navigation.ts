// Stand-in for SvelteKit's `$app/navigation` under Vitest, which runs the bare
// Svelte plugin and so has no SvelteKit module graph. Components that merely
// *reference* navigation can then be mounted and asserted against.
export const goto = async (_url: string) => {};
export const beforeNavigate = (_fn: unknown) => {};
export const afterNavigate = (_fn: unknown) => {};
export const invalidate = async (_x: unknown) => {};
export const invalidateAll = async () => {};
export const preloadData = async (_url: string) => {};
export const preloadCode = async (_url: string) => {};
