import { setSSRHandler, type StorageLike } from '@vueuse/core'

/** Every cookie a choice is kept in is named with this, apart from other programs' on 127.0.0.1. */
const PREFIX = 'mindflayer-'

/** A year, counted again each time the choice is made. */
const KEPT_FOR = 60 * 60 * 24 * 365

function name(key: string): string {
  return `${PREFIX}${encodeURIComponent(key)}`
}

/**
 * What this browser chose, kept in cookies: the colour mode, and the
 * sidebar's size and whether it is collapsed.
 *
 * Not localStorage, which is kept per origin: each `flayer ui` takes a port
 * of its own, so every launch would open on an empty one and forget what the
 * last was told. A cookie is kept per host, whatever the port (the reason the
 * server names its own `mf_<port>`), so a choice made under one launch is
 * there for the next, and for `make web/dev` on 127.0.0.1 too.
 *
 * The price is that the browser sends them with every request to 127.0.0.1,
 * whichever server listens there. So they hold small choices that say
 * nothing about anybody's work, and never something the server answered.
 */
export const cookies: StorageLike = {
  getItem(key) {
    const prefix = `${name(key)}=`
    const pair = document.cookie.split(/;\s*/).find(each => each.startsWith(prefix))
    if (pair === undefined) {
      return null
    }
    try {
      return decodeURIComponent(pair.slice(prefix.length))
    } catch {
      return null
    }
  },

  setItem(key, value) {
    document.cookie = `${name(key)}=${encodeURIComponent(value)}; Path=/; Max-Age=${KEPT_FOR}; SameSite=Strict`
  },

  removeItem(key) {
    document.cookie = `${name(key)}=; Path=/; Expires=Thu, 01 Jan 1970 00:00:00 GMT; SameSite=Strict`
  }
}

/**
 * Make the cookies where vueuse keeps whatever it is not handed a storage
 * for. Neither of the panel's choices can be handed one: Nuxt UI's plugin
 * reads the colour mode through a bare `useDark()`, and `UDashboardGroup`'s
 * `storage="local"` gives the sidebar a bare `useStorage`. vueuse asks for
 * the default as each of them starts, so this runs before any does.
 */
export function installPreferences(): void {
  setSSRHandler('getDefaultStorage', () => cookies)
}
