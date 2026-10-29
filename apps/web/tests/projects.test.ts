import { afterEach, describe, expect, it } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import type { Project } from '@/api/generated/Project'
import { fixture, render, texts, unmount } from './panel'

afterEach(unmount)

/** What a row is called in the table: its name, or its entry when it opens nothing. */
function title(project: Project): string {
  return project.name ?? project.entry ?? project.path
}

describe('the projects table', () => {
  it('lists every entry the registry names, in its order', async () => {
    const snapshot = fixture('workspace')

    const { page } = await render('/projects', snapshot)

    expect(texts(page, '[data-testid="name"]')).toEqual(snapshot.projects.map(title))
    expect(texts(page, '[data-testid="name"]')).toEqual(['api', 'web', 'docs', 'gone'])
  })

  it('shows each row\'s path, agents, share, counts per kind, health and why it is stale', async () => {
    const snapshot = fixture('workspace')

    const { page } = await render('/projects', snapshot)

    const rows = page.findAll('[data-testid="row"]')
    snapshot.projects.forEach((project, index) => {
      const row = rows[index]!
      expect(row.get('[data-testid="path"]').text()).toBe(project.path)
      expect(texts(row, '[data-testid="agent"]')).toEqual(project.agents)
      expect(row.get('[data-testid="share"]').text()).toBe(project.settings?.share ?? 'none')
      for (const counted of project.tally.kinds) {
        expect(row.get(`[data-testid="count-${counted.kind}"]`).text()).toBe(String(counted.count))
      }
      expect(row.get('[data-testid="health"]').text()).toBe(project.health)
      expect(row.get('[data-testid="stale"]').text()).toBe(project.stale?.message ?? 'no')
    })
    expect(texts(page, '[data-testid="health"]')).toEqual(['ok', 'attention', 'broken', 'stale'])
  })

  it('heads a column for every kind the snapshot names, in its order', async () => {
    const snapshot = fixture('workspace')

    const { page } = await render('/projects', snapshot)

    expect(page.findAll('[data-testid^="column-"]').map(column => column.attributes('data-testid'))).toEqual(
      snapshot.kinds.map(kind => `column-${kind}`)
    )
  })

  it('links each row to its project\'s page', async () => {
    const { page } = await render('/projects', fixture('workspace'))

    expect(page.findAll('[data-testid="name"]').map(link => link.attributes('href'))).toEqual([
      '/project?entry=api',
      '/project?entry=web',
      '/project?entry=docs',
      '/project?entry=gone'
    ])
  })
})

describe('sorting the projects', () => {
  it('sorts by a column, the other way, then back to the registry\'s order', async () => {
    const { page, router } = await render('/projects', fixture('workspace'))
    const sortBy = async () => {
      await page.get('[data-testid="sort-name"]').trigger('click')
      await flushPromises()
    }

    await sortBy()
    expect(texts(page, '[data-testid="name"]')).toEqual(['api', 'docs', 'gone', 'web'])
    expect(router.currentRoute.value.query).toEqual({ sort: 'name' })
    expect(page.get('[data-testid="sort-name"]').element.closest('th')!.getAttribute('aria-sort')).toBe('ascending')

    await sortBy()
    expect(texts(page, '[data-testid="name"]')).toEqual(['web', 'gone', 'docs', 'api'])
    expect(router.currentRoute.value.query).toEqual({ sort: 'name', order: 'desc' })
    expect(page.get('[data-testid="sort-name"]').element.closest('th')!.getAttribute('aria-sort')).toBe('descending')

    await sortBy()
    expect(texts(page, '[data-testid="name"]')).toEqual(['api', 'web', 'docs', 'gone'])
    expect(router.currentRoute.value.query).toEqual({})
    expect(page.get('[data-testid="sort-name"]').element.closest('th')!.hasAttribute('aria-sort')).toBe(false)
  })

  it('orders healths as the snapshot lists them, not as words', async () => {
    const snapshot = fixture('workspace')
    // Listed the other way round, as a core that ordered them so would.
    snapshot.healths.reverse()

    const { page } = await render('/projects?sort=health', snapshot)

    expect(texts(page, '[data-testid="health"]')).toEqual(['stale', 'broken', 'attention', 'ok'])
  })

  it('orders a kind\'s column by its count, a tie in the registry\'s order', async () => {
    const { page } = await render('/projects?sort=kind:skill&order=desc', fixture('workspace'))

    // web holds three skills, api and docs two each, gone none.
    expect(texts(page, '[data-testid="name"]')).toEqual(['web', 'api', 'docs', 'gone'])
    expect(texts(page, '[data-testid="count-skill"]')).toEqual(['3', '2', '2', '0'])
  })
})

describe('filtering the projects', () => {
  it('keeps the rows whose name, entry or path holds the text, and puts it in the address', async () => {
    const { page, router } = await render('/projects', fixture('workspace'))

    await page.get('input[data-testid="filter-text"]').setValue('DO')
    await flushPromises()

    expect(texts(page, '[data-testid="name"]')).toEqual(['docs'])
    expect(router.currentRoute.value.query).toEqual({ q: 'DO' })
  })

  it('keeps the rows in one health, said by a pressed button', async () => {
    const { page, router } = await render('/projects', fixture('workspace'))

    await page.get('[data-testid="filter-health-broken"]').trigger('click')
    await flushPromises()

    expect(texts(page, '[data-testid="name"]')).toEqual(['docs'])
    expect(router.currentRoute.value.query).toEqual({ health: 'broken' })
    expect(page.get('[data-testid="filter-health-broken"]').attributes('aria-pressed')).toBe('true')
  })

  it('opens with the filters and the order a link names, as after a reload', async () => {
    const { page } = await render('/projects?state=outdated', fixture('workspace'))

    expect(texts(page, '[data-testid="name"]')).toEqual(['web'])
    expect(page.get('[data-testid="filter-state"]').text()).toBe('holding outdated')

    unmount()
    const stale = await render('/projects?health=stale&sort=name', fixture('workspace'))
    expect(texts(stale.page, '[data-testid="name"]')).toEqual(['gone'])
  })

  it('says when nothing matches, and clears the filters', async () => {
    const { page, router } = await render('/projects?q=nowhere&health=ok', fixture('workspace'))

    expect(page.findAll('[data-testid="row"]')).toEqual([])
    await page.get('[data-testid="no-match"] button').trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.query).toEqual({})
    expect(page.findAll('[data-testid="row"]')).toHaveLength(4)
  })
})

describe('a workspace that links nothing', () => {
  it('names the commands that would link a project', async () => {
    const { page } = await render('/projects', fixture('unmade'))

    expect(page.find('[data-testid="table"]').exists()).toBe(false)
    expect(texts(page, '[data-testid="no-projects"] code')).toEqual([
      '$ mind init',
      '$ flayer link <PATH>',
      '$ flayer scan --suggest'
    ])
  })
})

describe('the keyboard', () => {
  it('reaches every control as a link, a button or a field', async () => {
    const { page } = await render('/projects', fixture('workspace'))

    for (const sort of page.findAll('[data-testid^="sort-"]')) {
      expect(sort.element.tagName).toBe('BUTTON')
      expect(sort.attributes('type')).toBe('button')
    }
    for (const name of page.findAll('[data-testid="name"]')) {
      expect(name.element.tagName).toBe('A')
      expect(name.attributes('href')).toBeTruthy()
    }
    expect(page.get('input[data-testid="filter-text"]').element.tagName).toBe('INPUT')
    for (const health of page.findAll('[data-testid^="filter-health-"]')) {
      expect(health.element.tagName).toBe('BUTTON')
    }
  })
})
