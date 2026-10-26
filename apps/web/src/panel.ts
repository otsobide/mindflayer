import type { App } from 'vue'
import { createRouter, createWebHistory, type Router, type RouterHistory } from 'vue-router'
import { routes } from 'vue-router/auto-routes'
import ui from '@nuxt/ui/vue-plugin'

export interface PanelOptions {
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
export function createPanel({ history = createWebHistory() }: PanelOptions = {}): Panel {
  const router = createRouter({ history, routes })
  return {
    router,
    install(app) {
      app.use(router)
      app.use(ui)
    }
  }
}
