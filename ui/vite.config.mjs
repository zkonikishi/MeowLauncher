import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// MeowLauncher 26.X.X renderer build.
// Output goes to ../dist/ui so the asar packer can put it next to dist/main.
export default defineConfig({
  base: './',
  plugins: [react()],
  build: {
    outDir: '../dist/ui',
    emptyOutDir: true,
    target: 'chrome126',
    assetsInlineLimit: 4096,
    chunkSizeWarningLimit: 2048,
    rollupOptions: {
      output: {
        manualChunks: {
          react: ['react', 'react-dom/client'],
        },
      },
    },
  },
  server: {
    port: 5273,
    strictPort: true,
  },
});
