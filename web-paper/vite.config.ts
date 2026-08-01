import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  base: process.env.GITHUB_ACTIONS ? "/TreeWork/" : "/",
  plugins: [react()],
  build: {
    outDir: "../dist/web-paper",
    emptyOutDir: true,
    assetsInlineLimit: 0,
  },
});
