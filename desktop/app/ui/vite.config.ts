import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// La UI solo se sirve desde Tauri: sin red, sin CDNs, todo empaquetado.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { target: 'es2022', sourcemap: false },
});
