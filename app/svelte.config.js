import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

export default {
  // Force runes mode so the whole app (and svelte-check) treats `$state`,
  // `$derived`, etc. as runes rather than ordinary identifiers.
  compilerOptions: { runes: true },
  preprocess: vitePreprocess(),
};
