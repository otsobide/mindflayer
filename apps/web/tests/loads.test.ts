import { afterEach, describe, expect, it } from 'vitest'
import { assignLink } from '@/routes'
import { fixture, render, texts, unmount } from './panel'

afterEach(unmount)

/*
 * The page of loads, drawn from what the CLI's suite wrote: the workspace
 * the matrix draws (tests/fixtures/assigning.json), which loads `team`, in
 * a home whose default workspace knows it, `team`, `kit` and `old`, gone
 * since; and the workspace holding something of everything
 * (workspace.json), whose `team` has gone.
 */

describe('the workspaces it loads', () => {
  it('lists each with where it is, what --from takes, and what it offers, as core counts and lists it', async () => {
    const snapshot = fixture('assigning')
    const [team] = snapshot.loaded

    const { page, router } = await render('/loads', snapshot)

    const load = page.get('[data-testid="load"]')
    expect(load.get('[data-testid="name"]').text()).toBe(team!.name)
    expect(load.get('[data-testid="entry"]').text()).toBe(team!.entry)
    expect(load.get('[data-testid="path"]').text()).toBe(team!.path)
    expect(load.get('[data-testid="origin"]').text()).toBe(team!.origin)
    expect(load.get('[data-testid="count"]').text()).toBe(`${team!.offers} to install`)
    expect(texts(load, '[data-testid="offer"]')).toEqual(
      snapshot.offers.filter(offer => offer.origin === team!.origin).map(offer => `${offer.kind}:${offer.name} ${offer.summary}`)
    )
    expect(load.findAll('[data-testid="offer"]')).toHaveLength(team!.offers)
    expect(load.get('[data-testid="assign"]').attributes('href')).toBe(router.resolve(assignLink(team!.origin)).href)
    expect(load.find('[data-testid="error"]').exists()).toBe(false)
  })

  it('says why one does not open, by its code, and offers nothing of it', async () => {
    const snapshot = fixture('workspace')
    const [team] = snapshot.loaded

    const { page } = await render('/loads', snapshot)

    const load = page.get('[data-testid="load"]')
    expect(load.get('[data-testid="name"]').text()).toBe(team!.entry)
    expect(load.get('[data-testid="unopened"]').text()).toBe('does not open')
    expect(load.get('[data-testid="error"]').text()).toBe(`${team!.error!.code}: ${team!.error!.message}`)
    expect(load.get('[data-testid="count"]').text()).toBe('0 to install')
    expect(load.find('[data-testid="offer"]').exists()).toBe(false)
    expect(load.find('[data-testid="assign"]').exists()).toBe(false)
  })

  it('names the command that would load one when it loads none', async () => {
    const snapshot = fixture('workspace')
    snapshot.loaded = []

    const { page } = await render('/loads', snapshot)

    expect(texts(page.get('[data-testid="no-loaded"]'), 'code')).toEqual([
      `$ flayer -W ${snapshot.workspace.path} load <PATH>`
    ])
  })

  it('lights the loads in the sidebar', async () => {
    const { page } = await render('/loads', fixture('assigning'))

    expect(page.get('[data-testid="sidebar"] nav a[aria-current="page"]').text()).toBe('Loads')
  })
})

describe('the workspaces this machine knows', () => {
  it('lists each in core\'s order, saying in words which is the default, this one, loaded, or one that does not open', async () => {
    const snapshot = fixture('assigning')

    const { page } = await render('/loads', snapshot)

    const rows = page.findAll('[data-testid="known-workspace"]')
    expect(rows.map(row => row.get('[data-testid="path"]').text())).toEqual(snapshot.known.map(known => known.path))
    expect(rows.map(row => texts(row, '[data-testid="badges"] li'))).toEqual([
      ['default', '0 projects'],
      ['this workspace', '5 projects, 1 stale'],
      ['loaded as ../team', '0 projects'],
      ['0 projects'],
      ['does not open']
    ])
    const old = snapshot.known[4]!
    expect(rows[4]!.find('[data-testid="name"]').exists()).toBe(false)
    expect(rows[4]!.get('[data-testid="stale"]').text()).toBe(old.stale)
  })

  it('says a machine knows none while nothing is made, and the command that makes the workspace', async () => {
    const snapshot = fixture('unmade')

    const { page } = await render('/loads', snapshot)

    expect(snapshot.known).toEqual([])
    expect(texts(page.get('[data-testid="no-known"]'), 'code')).toEqual([`$ flayer init ${snapshot.workspace.path}`])
  })
})
