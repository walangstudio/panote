import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath, URL } from "url";

const host = process.env.TAURI_DEV_HOST;
const isVitest = !!process.env.VITEST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  // Under Vitest use the bare Svelte plugin: SvelteKit's vitePreprocess needs a
  // full Vite environment and throws while preprocessing <style> blocks here.
  // Component styles are plain CSS, so there is nothing to preprocess anyway.
  plugins: [isVitest ? svelte({ preprocess: [] }) : sveltekit()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host ? "0.0.0.0" : false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
  // Crawl every component at startup. Otherwise a cold dep cache discovers deps
  // lazily and reloads the page mid-navigation, which timed out E2E specs.
  optimizeDeps: { entries: ["src/**/*.svelte"] },
  resolve: {
    alias: {
      $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
      // SvelteKit's virtual modules don't exist under the bare Svelte plugin.
      ...(isVitest
        ? {
            "$app/navigation": fileURLToPath(
              new URL("./src/lib/test-stubs/app-navigation.ts", import.meta.url),
            ),
            "$app/state": fileURLToPath(
              new URL("./src/lib/test-stubs/app-state.ts", import.meta.url),
            ),
          }
        : {}),
    },
    // Under Vitest, resolve Svelte's client build so components can be mounted
    // into a DOM and actually clicked.
    ...(isVitest ? { conditions: ["browser"] } : {}),
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
    coverage: {
      provider: "v8",
      // Without an explicit include, v8 reports on whatever it happened to load:
      // the `build/` bundle, `.svelte-kit` chunks and the Playwright mock, which
      // drown the real numbers.
      include: ["src/**/*.{ts,svelte}"],
      exclude: ["src/**/*.test.ts", "src/lib/test-stubs/**", "src/app.d.ts"],
    },
  },
}));
