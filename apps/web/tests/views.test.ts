import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import type { VueWrapper } from '@vue/test-utils'
import type { Health } from '@/api/generated/Health'
import { answering, commanding, fixture, plans, render, unmount, updates } from './panel'

/** Every `<style>` element anything adds while a test runs, which the CSP would refuse. */
let styles: Element[]
let watching: MutationObserver

beforeEach(() => {
  styles = []
  watching = new MutationObserver((records) => {
    for (const record of records) {
      for (const node of record.addedNodes) {
        if (node instanceof Element && node.tagName === 'STYLE') {
          styles.push(node)
        }
      }
    }
  })
  watching.observe(document, { childList: true, subtree: true })
})

afterEach(() => {
  watching.disconnect()
  unmount()
})

/**
 * Whether every icon on the page was drawn from the bundle: Iconify draws an
 * icon it holds as an `<svg>`, and nothing at all for one it would have had
 * to fetch, which the CSP refuses.
 */
function drawsEveryIcon(page: VueWrapper) {
  const icons = page.findAllComponents({ name: 'Icon' })
  expect(icons.length).toBeGreaterThan(0)
  for (const icon of icons) {
    expect(icon.element).toBeInstanceOf(SVGElement)
    expect((icon.element as SVGElement).innerHTML).not.toBe('')
  }
}

/**
 * The workspace fixture with `api` holding one artifact in every state the
 * snapshot names, and the projects one in every health, so every badge's
 * icon is drawn.
 */
function everyLook() {
  const snapshot = fixture('workspace')
  const api = snapshot.projects[0]!
  const [template] = api.artifacts
  api.artifacts = [...snapshot.states.map(state => ({ ...template!, name: `in-${state}`, state })), { ...template!, name: 'none', state: null }]
  snapshot.projects.forEach((project, index) => {
    project.health = snapshot.healths[index % snapshot.healths.length] as Health
  })
  return snapshot
}

describe('the views', () => {
  it.each(['/', '/projects', '/projects?sort=name', '/projects?sort=name&order=desc', '/project?entry=api', '/project?entry=docs', '/project?entry=gone', '/library', '/library?kind=rule', '/artifact?kind=skill&name=review&file=SKILL.md', '/artifact?kind=rule&name=style/comments', '/shelf', '/shelf?kind=skill', '/sources', '/discover', '/discover?root=%2Fhome%2Fyou%2FProjects', '/assign', '/assign?by=project'])(
    '%s draws every icon from the bundle and adds no style element',
    async (path) => {
      const { page } = await render(path, everyLook())

      drawsEveryIcon(page)
      expect(styles).toEqual([])
    }
  )
})

describe('the matrix, with a plan of what was marked', () => {
  it('draws every icon from the bundle, every lock and clash among them, and adds no style element', async () => {
    const { transport } = answering(fixture('assigning'), plans())
    const { page } = await render('/assign', transport)

    for (const cell of (plans()[2]!.query as { selection: { project: string, reference: string, origin: string }[] }).selection) {
      const label = `${cell.reference.replace(':', ' ')} from ${cell.origin} in ${cell.project}`
      await page.findAll('[data-testid="box"]').find(each => each.attributes('aria-label')!.split('; ')[0] === label)!.trigger('click')
    }
    await flushPromises()

    expect(page.findAll('[data-held="locked"]')).not.toHaveLength(0)
    expect(page.findAll('[data-held="conflict"]')).not.toHaveLength(0)
    drawsEveryIcon(page)
    expect(styles).toEqual([])
  })
})

describe('what a project an older marker marks offers', () => {
  it('draws every icon from the bundle, its dry run shown, and adds no style element', async () => {
    const { transport } = commanding(fixture('candidates'))
    const { page } = await render('/project?entry=legacy', transport)

    await page.get('[data-testid="migrate-dry"]').trigger('click')
    await flushPromises()
    document.querySelector<HTMLElement>('[data-testid="run"]')!.click()
    await vi.waitFor(() => expect(page.find('[data-testid="migrate-yes"]').exists()).toBe(true))

    drawsEveryIcon(page)
    expect(styles).toEqual([])
  })
})

describe('the page of updates', () => {
  it('draws every icon from the bundle, every state listed, and adds no style element', async () => {
    const { transport } = answering(fixture('updating'), updates())
    const { page } = await render('/updates', transport)

    expect(page.findAll('[data-testid="row"]')).not.toHaveLength(0)
    drawsEveryIcon(page)
    expect(styles).toEqual([])
  })
})
