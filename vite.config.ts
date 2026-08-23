import { defineConfig } from "vite";

// Porta fixa: o Tauri aponta o devUrl para ela e falhar é melhor que trocar sozinho.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // O cargo grava o binário em src-tauri/target enquanto o Vite observa a
      // árvore; nos Windows um .exe bloqueado derruba o watcher (EBUSY).
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: "esnext",
    emptyOutDir: true,
  },
});
