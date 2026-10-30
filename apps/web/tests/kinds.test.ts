import { afterEach, describe, expect, it } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import type { Kind } from '@/api/generated/Kind'
import type { Snapshot } from '@/api/generated/Snapshot'
import type { Does } from '@/api/generated/Does'
import type { Updates } from '@/api/generated/Updates'
import { answering, fixture, render, texts, unmount, updates } from './panel'

afterEach(unmount)

/** A kind this build of core does not have, as one added to `Kind::ALL` would arrive. */
const MCP = 'mcp' as Kind

/**
 * The workspace fixture as a core with one more kind would answer it: the
 * kind among the snapshot's, every tally counting it, and `api` holding one.
 * Nothing in the panel is told of it.
 */
function withAKindMore(): Snapshot {
  const snapshot = fixture('workspace')
  snapshot.kinds.push(MCP)
  snapshot.summary.held.kinds.push({ kind: MCP, count: 1 })
  snapshot.summary.library.kinds.push({ kind: MCP, count: 2 })
  snapshot.summary.shelf.kinds.push({ kind: MCP, count: 0 })
  for (const project of snapshot.projects) {
    project.tally.kinds.push({ kind: MCP, count: project.entry === 'api' ? 1 : 0 })
  }
  snapshot.library.artifacts.push({
    kind: MCP,
    name: 'linear',
    summary: 'Issues in Linear',
    state: null,
    agents: [],
    origin: null,
    targets: ['.mindflayer/library/mcp/linear.json'],
    issues: [],
    unread: null
  })
  snapshot.projects[0]!.artifacts.push({
    kind: MCP,
    name: 'github',
    summary: 'Issues and pull requests',
    state: 'current',
    agents: ['claude'],
    origin: 'workspace',
    targets: ['.mcp.json'],
    issues: [],
    unread: null
  })
  return snapshot
}

describe('a kind core adds', () => {
  it('gets a filter of the shelf, counted as the snapshot counts it', async () => {
    const { page } = await render('/shelf?kind=mcp', withAKindMore())

    const filter = page.get('[data-testid="filter-kind-mcp"]')
    expect(filter.attributes('aria-pressed')).toBe('true')
    expect(filter.get('[data-testid="kind-count"]').text()).toBe('0')
    expect(page.find('[data-testid="no-match"]').exists()).toBe(true)
  })

  it('gets a filter of the matrix', async () => {
    const { page } = await render('/assign?kind=mcp', withAKindMore())

    expect(page.get('[data-testid="filter-kind-mcp"]').attributes('aria-pressed')).toBe('true')
    expect(page.find('[data-testid="no-match"]').exists()).toBe(true)
  })

  it('gets a page of the library, counted as the snapshot counts it', async () => {
    const { page } = await render('/library?kind=mcp', withAKindMore())

    const link = page.get('[data-testid="kind-mcp"]')
    expect(link.attributes('aria-current')).toBe('page')
    expect(link.get('[data-testid="kind-count"]').text()).toBe('2')
    expect(texts(page, '[data-testid="name"]')).toEqual(['linear'])
  })

  it('gets a card on the overview, counted as the snapshot counts it', async () => {
    const { page } = await render('/', withAKindMore())

    const card = page.get('[data-testid="card-kind-mcp"]')
    expect(card.get('[data-testid="count"]').text()).toBe('1')
    expect(card.text()).toContain('2 in the library')
    expect(page.get('[data-testid="card-shelf"]').text()).toContain('0 mcp')
  })

  it('gets a column in the projects table, which sorts by its count', async () => {
    const { page } = await render('/projects', withAKindMore())

    expect(page.get('[data-testid="column-mcp"]').text()).toBe('mcp')
    expect(texts(page, '[data-testid="count-mcp"]')).toEqual(['1', '0', '0', '0'])

    for (const order of [['web', 'docs', 'gone', 'api'], ['api', 'web', 'docs', 'gone']]) {
      await page.get('[data-testid="sort-kind-mcp"]').trigger('click')
      await flushPromises()
      expect(texts(page, '[data-testid="name"]')).toEqual(order)
    }
  })

  it('gets a group on a project\'s page, and the commands for one a project holds none of', async () => {
    const { page } = await render('/project?entry=api', withAKindMore())

    const group = page.get('[data-testid="kind-mcp"]')
    expect(group.get('h2').text()).toBe('mcp 1')
    expect(texts(group, '[data-testid="artifact"] th > div:first-child')).toEqual(['github'])
    expect(texts(group, '[data-testid="targets"] li')).toEqual(['.mcp.json'])

    unmount()
    const docs = await render('/project?entry=docs', withAKindMore())
    expect(texts(docs.page, '[data-testid="kind-mcp"] code')).toContain('$ flayer -W /home/you/Projects install -p docs mcp:<NAME>')
  })
})

describe('a kind, and a step, core adds', () => {
  it('reach the page of updates, each said by its word, with no change offered for a step it knows none for', async () => {
    const answered = updates()
    const [api] = (answered[0]!.answer as Updates).projects
    api!.installations.push({
      kind: MCP,
      name: 'github',
      origin: 'workspace',
      state: 'outdated',
      does: 'resync' as Does,
      copies: ['.mcp.json'],
      left: null
    })

    const { page } = await render('/updates', answering(fixture('updating'), answered).transport)

    const row = page.findAll('[data-testid="state-outdated"] [data-testid="row"]')
      .find(each => each.get('[data-testid="name"]').text() === 'github')!
    expect(row.text()).toContain('mcp')
    expect(row.get('[data-testid="does"]').text()).toBe('resync: resync')
    expect(row.find('[data-testid="change"]').exists()).toBe(false)
  })
})
