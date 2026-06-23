import { defineConfig } from "vite";

// `esnext` so top-level `await` (used to init the wasm module) is preserved
// rather than rejected by the default browser target.
export default defineConfig({
  base: "./",
  build: { target: "esnext" },
  esbuild: { target: "esnext" },
});
