import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    // In sviluppo le chiamate a /api vengono inoltrate al BE (spec 003).
    proxy: {
      '/api': 'http://localhost:3000',
    },
  },
})
