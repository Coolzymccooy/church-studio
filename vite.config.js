import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

const isTauri = process.env.TAURI_ENV_PLATFORM !== undefined;

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],

  // Tauri: serve on a fixed port, disable HMR overlay in Tauri context
  server: {
    port: 5173,
    strictPort: true,
    // On Tauri, the Rust sidecar opens the WebView to this URL
    host: isTauri ? '127.0.0.1' : 'localhost',
  },

  // Tauri expects an ES module bundle with inlined assets
  build: {
    target: isTauri ? ['es2021', 'chrome105', 'safari13'] : ['es2020', 'chrome90'],
    minify: !process.env.TAURI_ENV_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },

  // Prevent Vite from obscuring Rust compile errors in the console
  clearScreen: false,
})
