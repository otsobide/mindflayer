import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'
import router from 'vue-router/vite'
import ui from '@nuxt/ui/vite'

const here = (path: string) => fileURLToPath(new URL(path, import.meta.url))

export default defineConfig({
  plugins: [
    // File routes: a page is a file under src/pages, so adding a view is
    // adding a file rather than editing a list of routes beside it.
    router({ dts: 'src/route-map.d.ts' }),
    vue(),
    ui({
      // Committed, so vue-tsc knows Nuxt UI's components on a fresh clone.
      // Not written by a test run, which only meets the components its
      // tests import and would leave out the rest.
      components: { dts: !process.env.VITEST && 'src/components.d.ts' },
      autoImport: { dts: 'src/auto-imports.d.ts' }
    })
  ],
  resolve: {
    alias: [
      { find: '@', replacement: here('./src') }
    ]
  },
  test: {
    environment: 'happy-dom',
    include: ['tests/**/*.test.ts']
  }
})
