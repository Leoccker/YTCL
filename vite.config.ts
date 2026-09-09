import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Porta fixa: o Tauri aponta o devUrl para ela e falhar alto e melhor que
// o Vite escolher outra porta silenciosamente e a janela abrir em branco.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/target/**"] },
  },
  build: {
    // WebKitGTK e o alvo mais restrito; nao vale transpilar abaixo disso.
    target: "es2022",
    sourcemap: false,
  },
});
