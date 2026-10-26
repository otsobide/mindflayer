import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'
import router from 'vue-router/vite'
import ui from '@nuxt/ui/vite'
import { colors } from './src/theme.ts'

/** Where `flayer ui --dev --port 7878` listens while the panel is worked on. */
const SERVER = 'http://127.0.0.1:7878'

const here = (path: string) => fileURLToPath(new URL(path, import.meta.url))

/**
 * Whether this run writes the declarations of Nuxt UI's components and
 * composables, which are committed so vue-tsc knows them on a fresh clone. A
 * build or the dev server does; a test run never does. vitest only meets the
 * components its tests import, and would write a list without the rest. The
 * build tests/dist.test.ts runs has none of vitest's variables, so it sets
 * this one instead. And a test that rewrote them would mend a stale file
 * behind the back of the test that looks for one (declarations.test.ts).
 */
const declarations = !process.env.VITEST && process.env.MINDFLAYER_PANEL_DTS !== 'off'

export default defineConfig({
  plugins: [
    // File routes: a page is a file under src/pages, so adding a view is
    // adding a file rather than editing a list of routes beside it.
    router({ dts: 'src/route-map.d.ts' }),
    vue(),
    ui({
      // The colour names components take, and what each stands for; main.css
      // turns the same palette into the variables the panel draws with.
      theme: { colors: Object.keys(colors).filter(name => name !== 'neutral') },
      ui: { colors },
      // Every Lucide icon a component names, and Nuxt UI's own, go into the
      // bundle: the server's CSP refuses the Iconify API, so an icon that is
      // not bundled is one that never shows.
      icon: { clientBundle: { scan: true } },
      components: { dts: declarations && 'src/components.d.ts' },
      autoImport: { dts: declarations && 'src/auto-imports.d.ts' }
    })
  ],
  resolve: {
    alias: [
      { find: '@', replacement: here('./src') },
      // Iconify's online build asks its API for any icon it was not given,
      // so its code holds the API's hosts; the offline build only draws what
      // it was handed. Swapping one for the other keeps that code out of the
      // bundle, rather than trusting the CSP to stop a request it makes.
      { find: /^@iconify\/vue$/, replacement: here('./src/overrides/iconify.ts') },
      // Nuxt UI's colours plugin writes its palette into a <style> element
      // at run time, which a CSP without 'unsafe-inline' refuses. main.css
      // declares the same variables in the stylesheet instead.
      {
        find: /^.*[\\/]@nuxt[\\/]ui[\\/]dist[\\/]runtime[\\/]plugins[\\/]colors\.js$/,
        replacement: here('./src/overrides/colors.ts')
      }
    ]
  },
  build: {
    // Nothing inlined as a data: URL: the CSP's default-src 'self' refuses
    // those for fonts and images alike, so every asset is a file of its own.
    assetsInlineLimit: 0
  },
  server: {
    host: '127.0.0.1',
    port: 5173,
    strictPort: true,
    proxy: {
      // changeOrigin, so the Host the server checks is its own address and
      // not this one's; the Origin stays the browser's, which --dev accepts.
      '/api': { target: SERVER, changeOrigin: true },
      // The launch link, so the token is traded for the cookie here, where
      // the panel runs, and the redirect lands on this server's `/`.
      '^/\\?token=': { target: SERVER, changeOrigin: true }
    }
  },
  test: {
    environment: 'happy-dom',
    include: ['tests/**/*.test.ts']
  }
})
