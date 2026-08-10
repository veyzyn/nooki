import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
// @ts-expect-error type error without @types/node package
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [react()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // Runtime data may live below the repository while developing. Watching it
      // is unnecessary and leaves node.exe directory handles behind on Windows,
      // which prevents server folders from being moved to the Recycle Bin.
      ignored: [
        "**/src-tauri/**",
        "**/Servers",
        "**/Servers/**",
        "**/Backups",
        "**/Backups/**",
        "**/.nooki-create-*",
        "**/.nooki-create-*/**",
      ],
    },
  },
}));
