import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri から使う前提の設定。開発サーバーは手元（localhost）でだけ待ち受ける。
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "es2022",
    sourcemap: false,
  },
});
