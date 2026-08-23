import { defineConfig } from "vite";

// Porta fixa: o Tauri aponta o devUrl para ela e falhar é melhor que trocar sozinho.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "esnext",
    emptyOutDir: true,
  },
});
