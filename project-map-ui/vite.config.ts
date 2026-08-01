import { fileURLToPath, URL } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  base: "./",
  plugins: [react()],
  build: {
    outDir: fileURLToPath(
      new URL(
        "../plugins/treework/assets/graph-panel",
        import.meta.url,
      ),
    ),
    emptyOutDir: true,
    assetsInlineLimit: 0,
    cssCodeSplit: false,
    rollupOptions: {
      output: {
        entryFileNames: "app.js",
        chunkFileNames: "vendor/[name].js",
        assetFileNames(assetInfo) {
          if (assetInfo.names.some((name) => name.endsWith(".css"))) {
            return "styles.css";
          }
          if (
            assetInfo.names.some((name) =>
              /\.(woff2?|ttf|otf)$/i.test(name),
            )
          ) {
            return "vendor/fonts/[name][extname]";
          }
          return "vendor/[name][extname]";
        },
      },
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: "./src/test/setup.ts",
    css: true,
  },
});
