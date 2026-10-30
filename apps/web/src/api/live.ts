import { computed, inject, reactive, readonly, type ComputedRef, type InjectionKey } from 'vue'
import type { QueryClient, QueryKey } from '@tanstack/vue-query'
import type { Holder } from './generated/Holder'
import type { LiveEvent, Transport, Unsubscribe } from './transport'

/** How the server watches the workspace, as far as the panel was told. */
export interface Live {
  /**
   * `waiting` until the server says, then `watching`, when a change on
   * disk is told as it happens, or `unwatched`, when one may not be.
   */
  state: 'waiting' | 'watching' | 'unwatched'
  /** Why not, when not. */
  reason?: string
}

export const liveKey: InjectionKey<Readonly<Live>> = Symbol('live')

/** How the server watches, for a view to say so. */
export function useLive(): Readonly<Live> {
  const live = inject(liveKey)
  if (!live) {
    throw new Error('the panel was started without its live updates')
  }
  return live
}

/**
 * How long an answer from the server stays fresh: for ever while the server
 * tells each change, since every change it tells marks what it reached as
 * stale; not at all otherwise, as before there was a stream, so a page asks
 * again on opening and the window on coming back into focus.
 */
export function useFreshness(): ComputedRef<number> {
  const live = useLive()
  return computed(() => (live.state === 'watching' ? Infinity : 0))
}

/**
 * The queries the server's events mark stale, by the first word of their
 * key (`src/api/snapshot.ts`, `src/api/artifact.ts`): the snapshot, and what
 * was read of an artifact. Named rather than every query, for one asked for
 * by hand: a scan (#77) reads the disk far past what is watched, and an
 * event asking for it again would run one on every change, every command
 * and every tab shown again. A query added later is followed only once it
 * is named here. Spelled here rather than imported, since both modules
 * import this one.
 */
const FOLLOWED: readonly unknown[] = ['snapshot', 'files', 'contents']

/**
 * Mark stale what the server's events follow: all of it, or what a change
 * of `scopes` reached. The views on screen then ask for it again.
 */
export function markStale(client: QueryClient, scopes?: Holder[]): Promise<void> {
  return client.invalidateQueries({
    predicate: ({ queryKey }) => FOLLOWED.includes(queryKey[0]) && (scopes === undefined || reached(queryKey, scopes))
  })
}

/**
 * Listen to the server's events for as long as the panel is open, one
 * subscription for the tab, and turn each into what Vue Query is to ask for
 * again, never a read of its own and never a scan: a change marks the
 * snapshot stale, and what was read of an artifact in a holder it reached,
 * which the views on screen then ask for again.
 *
 * The subscription is let go while the tab is hidden and taken again when it
 * is shown, which the server answers with `resync`: a browser opens at most
 * six HTTP/1.1 connections to one server, and every tab holding a stream
 * holds one of them, so a few tabs left open behind would leave the one in
 * front none to ask with.
 */
export function listen(transport: Transport, client: QueryClient, page: Document = document) {
  const live = reactive<Live>({ state: 'waiting' })
  let unsubscribe: Unsubscribe | undefined

  function told(event: LiveEvent) {
    switch (event.type) {
      case 'changed':
        void markStale(client, event.scopes)
        break
      // A command can change anything, and what it changed may lie where
      // nothing is watched: everything an event follows is asked for again.
      case 'command_finished':
      case 'resync':
        void markStale(client)
        break
      case 'command_started':
        break
      case 'watching':
        live.state = 'watching'
        live.reason = undefined
        break
      case 'unwatched':
        live.state = 'unwatched'
        live.reason = event.reason
        break
    }
  }

  function start() {
    unsubscribe ??= transport.subscribe(told)
  }

  function pause() {
    unsubscribe?.()
    unsubscribe = undefined
  }

  function shown() {
    if (page.visibilityState === 'hidden') {
      pause()
    } else {
      start()
    }
  }

  page.addEventListener('visibilitychange', shown)
  shown()
  return {
    live: readonly(live),
    stop() {
      page.removeEventListener('visibilitychange', shown)
      pause()
    }
  }
}

/**
 * Whether a change of `scopes` reaches what was asked under `key`: what was
 * read of an artifact, by the holder it was read from, and the snapshot,
 * which holds every holder.
 */
function reached(key: QueryKey, scopes: Holder[]): boolean {
  const named = key[1]
  if (typeof named !== 'object' || named === null || !('holder' in named)) {
    return true
  }
  const holder = named.holder as Holder
  return scopes.some(scope => same(scope, holder))
}

/**
 * Whether two holders are one: the workspace, or one project by its entry,
 * whether a task names it as its own or as one elsewhere.
 */
function same(a: Holder, b: Holder): boolean {
  return a.type === 'workspace' ? b.type === 'workspace' : b.type !== 'workspace' && b.entry === a.entry
}
