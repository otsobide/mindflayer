import { afterEach, describe, expect, it } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import { listen } from '@/api/live'
import type { LiveEvent, Query, Transport } from '@/api/transport'
import { createQueryClient } from '@/query'
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

  it('mark stale the snapshot and what was read of an artifact, never a query of another kind', async () => {
    const { transport, tell } = streaming()
    const client = createQueryClient()
    const { stop } = listen(transport, client)
    // A scan (#77) is asked for by hand, and reads far past what is watched.
    const scan = ['scan', { roots: ['/work'], depth: 2 }]
    const files = ['files', { holder: workspace, reference: 'skill:review' }]
    const stale = (key: unknown[]) => client.getQueryState(key)?.isInvalidated

    for (const event of [
      { type: 'changed', scopes: [workspace] },
      { type: 'command_finished', lines: ['flayer link beta'] },
      { type: 'resync' }
    ] as LiveEvent[]) {
      for (const key of [['snapshot'], files, scan]) {
        client.setQueryData(key, {})
      }
      await tell(event)

      expect([stale(['snapshot']), stale(files), stale(scan)], event.type).toEqual([true, true, false])
    }
    stop()
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

describe('a workspace the server cannot watch all of', () => {
  it('says so above every page, and asks for everything again on a refresh by hand', async () => {
    const { queries, transport, tell } = streaming()
    const { page, router } = await render('/', transport)
    expect(page.find('[data-testid="unwatched"]').exists()).toBe(false)

    await tell({ type: 'unwatched', reason: 'the system\'s limit on watched folders is reached' })

    const banner = page.get('[data-testid="unwatched"]')
    expect(banner.text()).toContain('the system\'s limit on watched folders is reached: refresh to see them.')
    const before = asked(queries, 'snapshot')
    await banner.get('button').trigger('click')
    await flushPromises()
    expect(asked(queries, 'snapshot')).toBe(before + 1)

    await router.push('/projects')
    await flushPromises()
    expect(page.find('[data-testid="unwatched"]').exists()).toBe(true)
  })

  it('stops saying so once the server watches everything again', async () => {
    const { transport, tell } = streaming()
    const { page } = await render('/', transport)
    await tell({ type: 'unwatched', reason: 'the system\'s limit on watched folders is reached' })

    await tell({ type: 'watching' })

    expect(page.find('[data-testid="unwatched"]').exists()).toBe(false)
  })
})
