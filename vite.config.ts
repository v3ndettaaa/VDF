/// <reference types="vitest" />
import { defineConfig } from "vite";

export default defineConfig({
  root: "ui",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
  },
  server: {
    port: 5173,
    strictPort: true,
  },
  test: {
    include: ["**/*.test.ts"],
    environment: "node",
  },
});
