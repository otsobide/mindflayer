import { afterEach, describe, expect, it } from 'vitest'
import { flushPromises, type DOMWrapper } from '@vue/test-utils'
import type { Snapshot } from '@/api/generated/Snapshot'
import { fixture, render, texts, unmount } from './panel'

afterEach(unmount)

/** Each project a row says holds a copy, and how that copy stands. */
function holders(row: DOMWrapper<Element>): string[][] {
  return row.findAll('[data-testid="holding"] li').map(item => [
    item.get('[data-testid="project"]').text(),
    item.get('[data-testid="state"]').text()
  ])
}

/** The names of `kind` the library holds, in the snapshot's order. */
function own(snapshot: Snapshot, kind: string): string[] {
  return snapshot.library.artifacts.filter(held => held.kind === kind).map(held => held.name)
}

describe('the library', () => {
  it('lists the first kind the snapshot names when the address names none, in its order', async () => {
    const snapshot = fixture('workspace')

    const { page } = await render('/library', snapshot)

    expect(texts(page, '[data-testid="name"]')).toEqual(own(snapshot, snapshot.kinds[0]!))
    expect(texts(page, '[data-testid="summary"]')).toEqual(
      snapshot.library.artifacts.filter(held => held.kind === snapshot.kinds[0]).map(held => held.summary)
    )
    expect(page.get('[data-testid="folder"]').text()).toBe(snapshot.library.directories[0]!.path)
  })

  it('has a page for every kind, each counted as the snapshot counts it', async () => {
    const snapshot = fixture('workspace')

    const { page } = await render('/library?kind=rule', snapshot)

    for (const kind of snapshot.kinds) {
      const link = page.get(`[data-testid="kind-${kind}"]`)
      expect(link.attributes('href')).toBe(`/library?kind=${kind}`)
      expect(link.get('[data-testid="kind-count"]').text())
        .toBe(String(snapshot.summary.library.kinds.find(counted => counted.kind === kind)!.count))
    }
    expect(page.get('[aria-current="page"][data-testid^="kind-"]').attributes('data-testid')).toBe('kind-rule')
    expect(texts(page, '[data-testid="name"]')).toEqual(own(snapshot, 'rule'))
  })

  it('opens each from its name, by its kind and name in the address', async () => {
    const { page } = await render('/library?kind=rule', fixture('workspace'))

    expect(page.findAll('[data-testid="name"]').map(link => link.attributes('href')))
      .toEqual(['/artifact?kind=rule&name=git/commits', '/artifact?kind=rule&name=style/comments'])
  })

  it('filters by name or summary, keeping the text in the address', async () => {
    const { page, router } = await render('/library?kind=rule', fixture('workspace'))

    await page.get('input[data-testid="filter-text"]').setValue('WHY, NOT')
    await flushPromises()

    expect(router.currentRoute.value.query).toEqual({ kind: 'rule', q: 'WHY, NOT' })
    expect(texts(page, '[data-testid="name"]')).toEqual(['style/comments'])

    await page.get('input[data-testid="filter-text"]').setValue('nothing at all')
    await flushPromises()
    expect(page.get('[data-testid="no-match"]').text()).toContain('No rule matches')
    await page.get('[data-testid="no-match"] button').trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.query).toEqual({ kind: 'rule' })
    expect(texts(page, '[data-testid="name"]')).toEqual(own(fixture('workspace'), 'rule'))
  })

  it('says which projects hold a copy of each, and how it stands there', async () => {
    const { page } = await render('/library?kind=skill', fixture('workspace'))

    const [lint, review] = page.findAll('[data-testid="row"]')
    expect(holders(lint!)).toEqual([['web', 'outdated']])
    expect(holders(review!)).toEqual([['api', 'current'], ['web', 'edited'], ['docs', 'current']])
    expect(review!.get('[data-testid="project"]').attributes('href')).toBe('/project?entry=api')
  })

  it('lists the holders core names, whatever origin a copy says or does not', async () => {
    const snapshot = fixture('workspace')
    const [template] = snapshot.projects[0]!.artifacts
    // Of the same name, even naming the library's origin, but no holder
    // core names: a project's own, and a copy of a rule of that name.
    snapshot.projects[0]!.artifacts.push(
      { ...template!, name: 'lint', state: 'authored', origin: null },
      { ...template!, kind: 'rule', name: 'lint', state: 'current', origin: 'workspace' }
    )
    snapshot.projects[2]!.artifacts.push({ ...template!, name: 'lint', state: 'current', origin: snapshot.library.origin })
    // A holder core names whose copy names no origin, as an install
    // recorded before origins were kept does not.
    const web = snapshot.projects.find(project => project.entry === 'web')!
    web.artifacts.find(copy => copy.kind === 'skill' && copy.name === 'lint')!.origin = null

    const { page } = await render('/library?kind=skill', snapshot)

    expect(holders(page.findAll('[data-testid="row"]')[0]!)).toEqual([['web', 'outdated']])
  })

  it('names the command that would give it a kind it holds none of', async () => {
    const snapshot = fixture('workspace')
    snapshot.library.artifacts = snapshot.library.artifacts.filter(held => held.kind !== 'rule')

    const { page } = await render('/library?kind=rule', snapshot)

    expect(texts(page.get('[data-testid="empty"]'), 'code')).toEqual([
      `$ flayer -W ${snapshot.workspace.path} add rule <NAME> <DESCRIPTION>`
    ])
  })

  it('names making the workspace first while it is not made', async () => {
    const snapshot = fixture('unmade')

    const { page } = await render('/library', snapshot)

    expect(texts(page.get('[data-testid="empty"]'), 'code')).toEqual([
      `$ flayer init ${snapshot.workspace.path}`,
      `$ flayer -W ${snapshot.workspace.path} add skill <NAME> <DESCRIPTION>`
    ])
  })

  it('says when the address names a kind this build does not know', async () => {
    const { page } = await render('/library?kind=agent', fixture('workspace'))

    expect(page.get('[data-testid="no-kind"]').text()).toContain('no kind called agent')
    expect(page.find('[data-testid="own"]').exists()).toBe(false)
  })

  it('lists what could not be read in it above what it holds', async () => {
    const snapshot = fixture('workspace')
    snapshot.library.failures = [{ code: 'unreadable_directory', message: '/home/you/Projects/.mindflayer/library/skills: cannot be listed' }]

    const { page } = await render('/library', snapshot)

    const failure = page.get('[data-testid="failure"]')
    expect(failure.text()).toContain('unreadable_directory')
    expect(failure.text()).toContain('/home/you/Projects/.mindflayer/library/skills: cannot be listed')
  })

  it('lights the library in the sidebar', async () => {
    const { page } = await render('/library', fixture('workspace'))

    expect(page.get('[data-testid="sidebar"] nav a[aria-current="page"]').text()).toBe('Library')
  })
})

describe('the loaded workspaces', () => {
  it('lists each with where it is, what it offers, what --from takes, and why it does not open', async () => {
    const snapshot = fixture('workspace')
    const [team] = snapshot.loaded

    const { page } = await render('/library', snapshot)

    const row = page.get('[data-testid="load"]')
    expect(row.get('th').text()).toBe(team!.entry)
    expect(row.text()).toContain(team!.path)
    expect(row.get('[data-testid="offers"]').text()).toBe(String(team!.offers))
    expect(row.get('[data-testid="origin"]').text()).toBe(team!.origin)
    expect(row.get('[data-testid="error"]').text()).toBe(`${team!.error!.code}: ${team!.error!.message}`)
  })

  it('names a loaded workspace that opens by its name, beside its entry', async () => {
    const snapshot = fixture('workspace')
    snapshot.loaded[0] = { ...snapshot.loaded[0]!, name: 'team', offers: 3, error: null }

    const { page } = await render('/library', snapshot)

    const row = page.get('[data-testid="load"]')
    expect(texts(row, 'th div')).toEqual(['team', '../team'])
    expect(row.get('[data-testid="offers"]').text()).toBe('3')
    expect(row.get('[data-testid="error"]').text()).toBe('yes')
  })

  it('names the command that would load one when it loads none', async () => {
    const snapshot = fixture('workspace')
    snapshot.loaded = []

    const { page } = await render('/library', snapshot)

    expect(texts(page.get('[data-testid="no-loaded"]'), 'code')).toEqual([
      `$ flayer -W ${snapshot.workspace.path} load <PATH>`
    ])
  })
})
