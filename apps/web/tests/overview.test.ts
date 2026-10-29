import { afterEach, describe, expect, it } from 'vitest'
import { TransportError } from '@/api/transport'
import type { Kind } from '@/api/generated/Kind'
import type { Said } from '@/api/generated/Said'
import type { State } from '@/api/generated/State'
import type { Tally } from '@/api/generated/Tally'
import { answering, fixture, refusing, render, texts, unmount } from './panel'

afterEach(unmount)

function ofKind(tally: Tally, kind: Kind): number {
  return tally.kinds.find(counted => counted.kind === kind)!.count
}

function inState(tally: Tally, state: State): number {
  return tally.states.find(counted => counted.state === state)!.count
}

describe('the overview', () => {
  it('asks the server for the snapshot, once', async () => {
    const { queries, transport } = answering(fixture('workspace'))

    await render('/', transport)

    expect(queries).toEqual([{ type: 'snapshot' }])
  })

  it('shows each count as the snapshot counts it', async () => {
    const snapshot = fixture('workspace')
    const { summary } = snapshot

    const { page } = await render('/', snapshot)

    const count = (card: string) => page.get(`[data-testid="card-${card}"] [data-testid="count"]`).text()
    expect(count('projects')).toBe(String(summary.projects))
    expect(count('stale')).toBe(String(summary.stale))
    expect(count('outdated')).toBe(String(inState(summary.held, 'outdated')))
    expect(count('edited')).toBe(String(inState(summary.held, 'edited')))
    expect(count('shelf')).toBe(String(summary.shelf.total))
    for (const kind of snapshot.kinds) {
      expect(count(`kind-${kind}`), kind).toBe(String(ofKind(summary.held, kind)))
      expect(page.get(`[data-testid="card-kind-${kind}"]`).text()).toContain(`${ofKind(summary.library, kind)} in the library`)
    }
    // The fixture's own numbers, so a count that moved shows here as well.
    expect([summary.projects, summary.stale, inState(summary.held, 'outdated'), inState(summary.held, 'edited')]).toEqual([3, 1, 1, 1])
  })

  it('lays a card out for every kind the snapshot names, in its order', async () => {
    const snapshot = fixture('workspace')

    const { page } = await render('/', snapshot)

    const kinds = page.findAll('[data-testid^="card-kind-"]').map(card => card.attributes('data-testid'))
    expect(kinds).toEqual(snapshot.kinds.map(kind => `card-kind-${kind}`))
  })

  it('links each count to the projects it is about', async () => {
    const { page } = await render('/', fixture('workspace'))

    const link = (card: string) => page.get(`[data-testid="card-${card}"] a`).attributes('href')
    expect(link('projects')).toBe('/projects')
    expect(link('stale')).toBe('/projects?health=stale')
    expect(link('outdated')).toBe('/projects?state=outdated')
    expect(link('edited')).toBe('/projects?state=edited')
  })

  it('says a count that asks to be looked at in words as well as colour', async () => {
    const { page } = await render('/', fixture('workspace'))

    const stale = page.get('[data-testid="card-stale"]')
    expect(stale.text()).toContain('stale')
    expect(stale.text()).toContain('opening no project')
  })

  it('names the workspace, where it is and how it was chosen', async () => {
    const snapshot = fixture('workspace')

    const { page } = await render('/', snapshot)

    expect(page.get('[data-testid="overview"]').text()).toContain(snapshot.workspace.name)
    expect(page.get('[data-testid="workspace-path"]').text()).toBe(snapshot.workspace.path)
    expect(page.get('[data-testid="overview"]').text()).toContain(snapshot.workspace.reason)
    expect(page.find('[data-testid="unmade"]').exists()).toBe(false)
  })
})

describe('the warnings panel', () => {
  it('lists every failure the snapshot collected, under what it is about', async () => {
    const snapshot = fixture('workspace')
    const web = snapshot.projects.find(project => project.entry === 'web')!
    const gone = snapshot.projects.find(project => project.entry === 'gone')!
    const said = ({ code, message }: Said) => `${code}: ${message}`

    const { page } = await render('/', snapshot)

    const groups = page.findAll('[data-testid="warning-group"]')
    expect(groups.map(group => group.get('h3').text())).toEqual(['web', 'gone', 'load ../team'])
    expect(groups.map(group => texts(group, '[data-testid="warning"]'))).toEqual([
      web.failures.map(said),
      [said(gone.stale!)],
      [said(snapshot.loaded[0]!.error!)]
    ])
    expect(web.failures.map(failure => failure.code)).toEqual(['copies_diverged'])
    // A project's failures link to its page.
    expect(groups[0]!.get('a').attributes('href')).toBe('/project?entry=web')
  })

  it('says so when nothing failed', async () => {
    const { page } = await render('/', fixture('unmade'))

    expect(page.findAll('[data-testid="warning-group"]')).toEqual([])
    expect(page.get('[data-testid="no-warnings"]').text()).toContain('nothing failed')
  })
})

describe('an empty workspace', () => {
  it('says it is not made yet, and names the commands that would fill it', async () => {
    const { page } = await render('/', fixture('unmade'))

    expect(page.get('[data-testid="unmade"]').text()).toContain('Not made yet')
    expect(texts(page, '[data-testid="start"] code')).toEqual([
      '$ mind init',
      '$ flayer link <PATH>',
      '$ flayer scan --suggest',
      '$ flayer add <KIND> <NAME> <DESCRIPTION>',
      '$ flayer gather git <URL>'
    ])
  })

  it('names no command for what is already there', async () => {
    const { page } = await render('/', fixture('workspace'))

    expect(page.find('[data-testid="start"]').exists()).toBe(false)
  })
})

describe('a snapshot that does not come', () => {
  it('says the server is still being asked', async () => {
    const pending = { query: () => new Promise(() => {}), send: () => new Promise(() => {}), subscribe: () => () => {} }

    const { page } = await render('/', pending as never)

    expect(page.get('[data-testid="reading"]').attributes('role')).toBe('status')
    expect(page.get('[data-testid="reading"]').text()).toBe('Reading the workspace')
  })

  it('says why the server would not answer', async () => {
    const refused = new TransportError(401, 'this browser does not hold the launch token: open the link `flayer ui` printed')

    const { page } = await render('/', refusing(refused))

    expect(page.get('[data-testid="unread"]').text()).toContain('this browser does not hold the launch token')
  })
})
