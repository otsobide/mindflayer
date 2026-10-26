import { ref } from 'vue'
import { defineStore } from 'pinia'

/**
 * The state of the panel's frame, and nothing the server knows.
 *
 * Pinia holds only what the browser decides, such as whether a menu is
 * open. Whatever comes from the server lives in Vue Query's cache, which
 * knows when it is stale and asks again; a copy kept here would not.
 */
export const useShell = defineStore('shell', () => {
  /** The sidebar on a narrow screen, where it opens over the page. */
  const sidebarOpen = ref(false)
  /** The sidebar on a wide one, reduced to its icons. */
  const sidebarCollapsed = ref(false)

  return { sidebarOpen, sidebarCollapsed }
})
