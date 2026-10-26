/**
 * `@iconify/vue` as Nuxt UI uses it, drawing only the icons the build
 * bundled (vite.config.ts points the package's name here).
 *
 * The online build fetches any icon it has not been given from the Iconify
 * API, which the server's CSP refuses; this one has no code that could ask.
 * It lacks the two things Nuxt UI's `Icon` relies on beyond `Icon` itself,
 * `iconLoaded`, and finding `lucide-check` under the `lucide:check` the
 * bundle registers, so they are added here.
 */
import { addIcon as store, Icon } from '@iconify/vue/offline'
import type { IconifyIcon } from '@iconify/vue/offline'

const loaded = new Set<string>()

/**
 * Register an icon under its `prefix:name` and its `prefix-name`, the two
 * spellings Nuxt UI looks icons up by (`i-lucide-check` arrives as
 * `lucide-check`).
 */
export function addIcon(name: string, data: IconifyIcon): boolean {
  const names = [name, name.replace(':', '-')]
  for (const each of names) {
    store(each, data)
    loaded.add(each)
  }
  return true
}

/** Whether an icon can be drawn at once, which here is whether it was bundled. */
export function iconLoaded(name: string): boolean {
  return loaded.has(name)
}

export { Icon }
