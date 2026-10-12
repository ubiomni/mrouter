import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import path from 'path';

export default defineConfig({
  base: process.env.VITE_BASE_PATH || '/',
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, 'src'),
    },
  },
  server: {
    port: 3000,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:4445',
        changeOrigin: true,
      },
      '/v1': {
        target: 'http://127.0.0.1:4444',
        changeOrigin: true,
        timeout: 120000,
      },
    },
  },
});
