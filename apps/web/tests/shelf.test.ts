import { afterEach, describe, expect, it } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import { instant, when } from '@/time'
import { fixture, render, texts, unmount } from './panel'

afterEach(unmount)

describe('the shelf', () => {
  it('lists everything gathered, in the snapshot\'s order, with the origin --from takes', async () => {
    const snapshot = fixture('workspace')

    const { page } = await render('/shelf', snapshot)

    expect(texts(page, '[data-testid="name"]')).toEqual(snapshot.shelf.map(shelved => shelved.name))
    expect(texts(page, '[data-testid="summary"]')).toEqual(snapshot.shelf.map(shelved => shelved.summary))
    expect(texts(page, '[data-testid="origin"]')).toEqual(snapshot.shelf.map(shelved => shelved.origin))
    expect(texts(page, '[data-testid="path"]')).toEqual(snapshot.shelf.map(shelved => shelved.path))
    expect(texts(page, '[data-testid="kind"]')).toEqual(snapshot.shelf.map(shelved => shelved.kind))
  })

  it('says the origin core gives, a branch asked for among it, never one of its own', async () => {
    const snapshot = fixture('workspace')
    snapshot.shelf[1]!.source.reference = 'main'
    snapshot.shelf[1]!.origin = 'https://github.com/acme/skills#main'

    const { page } = await render('/shelf', snapshot)

    expect(texts(page, '[data-testid="origin"]')).toEqual(['https://github.com/acme/skills', 'https://github.com/acme/skills#main'])
  })

  it('says the commit each was taken at, and when, in this browser\'s words', async () => {
    const snapshot = fixture('workspace')
    const [format] = snapshot.shelf

    const { page } = await render('/shelf', snapshot)

    const revision = page.get('[data-testid="revision"] code')
    expect(revision.text()).toBe(format!.revision!.slice(0, 7))
    expect(revision.attributes('title')).toBe(format!.revision)
    const gathered = page.get('[data-testid="gathered"]')
    expect(gathered.text()).toBe(when(format!.gathered_at))
    expect(gathered.attributes('datetime')).toBe(instant(format!.gathered_at))
  })

  it('names the install that takes one of them, with the workspace and --from', async () => {
    const snapshot = fixture('workspace')

    const { page } = await render('/shelf', snapshot)

    expect(texts(page, '[data-testid="commands"] code')).toEqual([
      `$ flayer -W ${snapshot.workspace.path} install -p <PROJECT> <KIND>:<NAME> --from <ORIGIN>`
    ])
  })

  it('filters by name, summary or origin, and by kind, counted as the snapshot counts them', async () => {
    const snapshot = fixture('workspace')

    const { page, router } = await render('/shelf', snapshot)

    for (const kind of snapshot.kinds) {
      expect(page.get(`[data-testid="filter-kind-${kind}"] [data-testid="kind-count"]`).text())
        .toBe(String(snapshot.summary.shelf.kinds.find(counted => counted.kind === kind)!.count))
    }
    await page.get('input[data-testid="filter-text"]').setValue('ISSUES')
    await flushPromises()
    expect(router.currentRoute.value.query).toEqual({ q: 'ISSUES' })
    expect(texts(page, '[data-testid="name"]')).toEqual(['triage'])

    await page.get('[data-testid="filter-kind-rule"]').trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.query).toEqual({ q: 'ISSUES', kind: 'rule' })
    expect(page.get('[data-testid="filter-kind-rule"]').attributes('aria-pressed')).toBe('true')
    expect(page.get('[data-testid="no-match"]').text()).toContain('Nothing on the shelf matches')

    await page.get('[data-testid="no-match"] button').trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.query).toEqual({})
    expect(texts(page, '[data-testid="name"]')).toEqual(snapshot.shelf.map(shelved => shelved.name))
  })

  it('names the gather that would fill it while it is empty', async () => {
    const snapshot = fixture('unmade')

    const { page } = await render('/shelf', snapshot)

    expect(texts(page.get('[data-testid="empty"]'), 'code')).toEqual([
      `$ flayer init ${snapshot.workspace.path}`,
      `$ flayer -W ${snapshot.workspace.path} gather git <URL>`
    ])
  })

  it('lights the shelf in the sidebar', async () => {
    const { page } = await render('/shelf', fixture('workspace'))

    expect(page.get('[data-testid="sidebar"] nav a[aria-current="page"]').text()).toBe('Shelf')
  })
})
