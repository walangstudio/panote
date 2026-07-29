// Stand-in for SvelteKit's `$app/state` under Vitest, which runs the bare
// Svelte plugin and so has no SvelteKit module graph. `page.url` is all
// components in this codebase read, so a fixed URL is enough to mount them.
export const page = { url: new URL("http://localhost/") };
