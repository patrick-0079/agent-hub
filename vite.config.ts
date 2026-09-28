import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed dev port and ignores src-tauri when watching.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // src-tauri 由 cargo 自己 watch；原子写产生的临时文件/目录会让 Vite
      // watcher 抛 EBUSY 并直接退出，这里一并忽略
      ignored: [
        "**/src-tauri/**",
        "**/*.tmp",
        "**/*.tmpdir/**",
        "**/.*.tmpdir/**",
        "**/.~*",
        "**/*.bak",
      ],
    },
  },
  build: {
    target: "chrome110",
    sourcemap: false,
  },
});