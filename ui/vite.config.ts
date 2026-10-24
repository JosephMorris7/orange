import { defineConfig } from 'vite';
import { resolve } from 'node:path';

export default defineConfig({
  base: './',
  clearScreen: false,
  server: { strictPort: true, host: '127.0.0.1', port: 5173 },
  build: {
    outDir: 'dist',
    target: 'es2022',
    chunkSizeWarningLimit: 900,
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
        wheel: resolve(__dirname, 'wheel.html'),
        tool: resolve(__dirname, 'tool.html'),
        shelf: resolve(__dirname, 'shelf.html'),
        splash: resolve(__dirname, 'splash.html'),
      },
    },
  },
});
