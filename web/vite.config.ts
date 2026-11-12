import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// En développement, l'API Rust tourne sur :8080 ; Vite relaie /api.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: { "/api": "http://127.0.0.1:8080" },
  },
  build: { outDir: "dist", sourcemap: false },
});
