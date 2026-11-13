import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import { fileURLToPath, URL } from "node:url";

// Tauri expects a fixed port and must see Rust errors in the terminal.
export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
  // The browser mock loads the SDK's example plugin from sdk/examples.
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] }, fs: { allow: [".", "../../sdk"] } },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "es2022",
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    // The splash window is a page of its own (splash.html).
    rollupOptions: {
      input: {
        index: fileURLToPath(new URL("./index.html", import.meta.url)),
        splash: fileURLToPath(new URL("./splash.html", import.meta.url)),
      },
    },
  },
  test: { environment: "jsdom" },
});
