import type { App } from 'vue'
import { createRouter, createWebHistory, type Router, type RouterHistory } from 'vue-router'
import { routes } from 'vue-router/auto-routes'
import { createPinia } from 'pinia'
import { VueQueryPlugin } from '@tanstack/vue-query'
import ui from '@nuxt/ui/vue-plugin'
import { listen, liveKey, settleKey } from './api/live'
import { transportKey, type Transport } from './api/transport'
import { installColorMode } from './color-mode'
import { installPreferences } from './preferences'
import { createQueryClient } from './query'

export interface PanelOptions {
  /** How the panel reaches the server; a test hands it a fake. */
  transport: Transport
  /** The browser's address bar, unless a test keeps routes in memory. */
  history?: RouterHistory
}

/** Everything the panel installs on its app, and the router it routes with. */
export interface Panel {
  install(app: App): void
  router: Router
}

/**
 * The panel as a plugin: the app in main.ts and the shell test install the
 * same one, so the test renders what a browser does.
 */
export function createPanel({ transport, history = createWebHistory() }: PanelOptions): Panel {
  const router = createRouter({ history, routes })
  return {
    router,
    install(app) {
      // Before Nuxt UI, whose plugin starts watching the colour mode, and
      // the colour mode before it, so that both keep it in the cookie.
      installPreferences()
      installColorMode()
      app.provide(transportKey, transport)
      app.use(router)
      app.use(createPinia())
      const queryClient = createQueryClient()
      app.use(VueQueryPlugin, { queryClient })
      // One subscription for the app, which is the tab, let go with it.
      const { live, settle, stop } = listen(transport, queryClient)
      app.provide(liveKey, live)
      app.provide(settleKey, settle)
      app.onUnmount(stop)
      app.use(ui)
    }
  }
}
