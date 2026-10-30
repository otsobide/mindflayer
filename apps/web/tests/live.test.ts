import { afterEach, describe, expect, it } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import type { LiveEvent, Query, Transport } from '@/api/transport'
import { answering, fixture, render, unmount } from './panel'

afterEach(() => {
  unmount()
  shown('visible')
})

/**
 * A transport answering the workspace fixture as `answering` does, which
 * hands the test what the panel subscribed with, so it can tell the panel
 * what the server's stream would.
 */
function streaming() {
  const { queries, transport } = answering(fixture('workspace'))
  const listeners: ((event: LiveEvent) => void)[] = []
  const subscriptions = { taken: 0, let: 0 }
  const streamed = {
    ...transport,
    subscribe(listener: (event: LiveEvent) => void) {
      subscriptions.taken += 1
      listeners.push(listener)
      return () => {
        subscriptions.let += 1
        listeners.splice(listeners.indexOf(listener), 1)
      }
    }
  } as Transport
  return {
    queries,
    subscriptions,
    transport: streamed,
    async tell(event: LiveEvent) {
      for (const listener of [...listeners]) {
        listener(event)
      }
      await flushPromises()
    }
  }
}

/** How many times `queries` holds one of `type`. */
function asked(queries: Query[], type: Query['type']): number {
  return queries.filter(query => query.type === type).length
}

/** Show or hide the tab, as the browser does when another is chosen. */
function shown(state: DocumentVisibilityState) {
  Object.defineProperty(document, 'visibilityState', { value: state, configurable: true })
  document.dispatchEvent(new Event('visibilitychange'))
}

const workspace = { type: 'workspace' } as const

describe('live updates', () => {
  it('take one subscription for the tab, let go when the panel closes', async () => {
    const { subscriptions, transport } = streaming()

    const { router } = await render('/', transport)
    await router.push('/projects')
    await flushPromises()

    expect(subscriptions).toEqual({ taken: 1, let: 0 })
    unmount()
    expect(subscriptions).toEqual({ taken: 1, let: 1 })
  })

  it('ask for the snapshot again when the server says the workspace changed', async () => {
    const { queries, transport, tell } = streaming()
    await render('/', transport)
    const before = asked(queries, 'snapshot')

    await tell({ type: 'changed', scopes: [workspace] })

    expect(asked(queries, 'snapshot')).toBe(before + 1)
  })

  it('ask for nothing on moving between pages while the server watches, and on each as before when it does not', async () => {
    const { queries, transport, tell } = streaming()
    const { router } = await render('/', transport)
    await tell({ type: 'watching' })
    const before = asked(queries, 'snapshot')

    await router.push('/projects')
    await flushPromises()
    await router.push('/shelf')
    await flushPromises()
    expect(asked(queries, 'snapshot')).toBe(before)

    await tell({ type: 'unwatched', reason: 'the system\'s limit on watched folders is reached' })
    await router.push('/sources')
    await flushPromises()
    expect(asked(queries, 'snapshot')).toBe(before + 1)
  })

  it('ask for everything again on a resync and once a command finished, and nothing when one starts', async () => {
    const { queries, transport, tell } = streaming()
    await render('/', transport)
    const before = asked(queries, 'snapshot')

    await tell({ type: 'command_started', lines: ['flayer link beta'] })
    expect(asked(queries, 'snapshot')).toBe(before)
    await tell({ type: 'command_finished', lines: ['flayer link beta'] })
    expect(asked(queries, 'snapshot')).toBe(before + 1)
    await tell({ type: 'resync' })
    expect(asked(queries, 'snapshot')).toBe(before + 2)
  })

  it('ask for what an artifact holds again only when a change reaches its holder', async () => {
    const { queries, transport, tell } = streaming()
    await render('/artifact?kind=skill&name=review', transport)
    await tell({ type: 'watching' })
    const files = asked(queries, 'files')
    const contents = asked(queries, 'contents')
    const snapshots = asked(queries, 'snapshot')
    expect(files).toBeGreaterThan(0)

    await tell({ type: 'changed', scopes: [{ type: 'project', entry: 'api' }] })
    expect(asked(queries, 'snapshot')).toBe(snapshots + 1)
    expect(asked(queries, 'files')).toBe(files)
    expect(asked(queries, 'contents')).toBe(contents)

    await tell({ type: 'changed', scopes: [workspace] })
    expect(asked(queries, 'files')).toBe(files + 1)
    expect(asked(queries, 'contents')).toBe(contents + 1)
  })

  it('let the stream go while the tab is hidden, and take it again once shown', async () => {
    const { subscriptions, transport } = streaming()
    await render('/', transport)

    shown('hidden')
    expect(subscriptions).toEqual({ taken: 1, let: 1 })
    shown('visible')
    expect(subscriptions).toEqual({ taken: 2, let: 1 })
  })
})
