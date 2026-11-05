import { setSSRHandler, type StorageLike } from '@vueuse/core'

/**
 * Every cookie a choice is kept in is named with this, apart from other
 * programs' on 127.0.0.1.
 */
const PREFIX = 'mindweaver-'

/**
 * What a Mindflayer build's panel named them with (D-26). A choice made
 * under it is read once, where none is kept under the new name, and moved
 * to it: the browser keeps its colour mode and its sidebar across the
 * rename rather than forget them, and the old cookie goes, so it is read no
 * more than that once.
 */
const FLAYER_PREFIX = 'mindflayer-'

/** A year, counted again each time the choice is made. */
const KEPT_FOR = 60 * 60 * 24 * 365

function name(key: string, prefix: string = PREFIX): string {
  return `${prefix}${encodeURIComponent(key)}`
}

/** The cookie called `named`, as it was set: `null` when there is none. */
function read(named: string): string | null {
  const prefix = `${named}=`
  const pair = document.cookie.split(/;\s*/).find(each => each.startsWith(prefix))
  if (pair === undefined) {
    return null
  }
  try {
    return decodeURIComponent(pair.slice(prefix.length))
  } catch {
    return null
  }
}

function forget(named: string): void {
  document.cookie = `${named}=; Path=/; Expires=Thu, 01 Jan 1970 00:00:00 GMT; SameSite=Strict`
}

/**
 * What this browser chose, kept in cookies: the colour mode, and the
 * sidebar's size and whether it is collapsed.
 *
 * Not localStorage, which is kept per origin: each `mindweaver ui` takes a port
 * of its own, so every launch would open on an empty one and forget what the
 * last was told. A cookie is kept per host, whatever the port (the reason the
 * server names its own `mw_<port>`), so a choice made under one launch is
 * there for the next, and for `make web/dev` on 127.0.0.1 too.
 *
 * The price is that the browser sends them with every request to 127.0.0.1,
 * whichever server listens there. So they hold small choices that say
 * nothing about anybody's work, and never something the server answered.
 */
export const cookies: StorageLike = {
  getItem(key) {
    const kept = read(name(key))
    if (kept !== null) {
      return kept
    }
    const older = read(name(key, FLAYER_PREFIX))
    if (older !== null) {
      cookies.setItem(key, older)
      forget(name(key, FLAYER_PREFIX))
    }
    return older
  },

  setItem(key, value) {
    document.cookie = `${name(key)}=${encodeURIComponent(value)}; Path=/; Max-Age=${KEPT_FOR}; SameSite=Strict`
  },

  removeItem(key) {
    forget(name(key))
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
