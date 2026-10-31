import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import type { VueWrapper } from '@vue/test-utils'
import type { Health } from '@/api/generated/Health'
import { answering, answers, applying, commanded, commanding, drafts, fixture, plans, render, unmount, updates, type Answered } from './panel'

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

describe('one of the library\'s own, about to be removed', () => {
  it('draws every icon from the bundle, its dry run shown, and adds no style element', async () => {
    const { transport } = commanding(fixture('workspace'), commanded(), answers())
    const { page } = await render('/artifact?kind=skill&name=review', transport)

    await page.get('[data-testid="remove-dry"]').trigger('click')
    await flushPromises()
    document.querySelector<HTMLElement>('[data-testid="run"]')!.click()
    await vi.waitFor(() => expect(page.find('[data-testid="remove-yes"]').exists()).toBe(true))

    drawsEveryIcon(page)
    expect(styles).toEqual([])
  })
})

describe('authoring in the library', () => {
  it.each(['/edit?kind=skill&name=review', '/new?kind=skill'])('%s draws every icon from the bundle and adds no style element', async (path) => {
    const { transport } = answering(fixture('workspace'), [...answers(), ...drafts()])
    const { page } = await render(path, transport)

    drawsEveryIcon(page)
    expect(styles).toEqual([])
  })
})

/** Press `key` where the focus is, as a keyboard would. */
function press(key: string, modifiers: KeyboardEventInit = {}) {
  const at: EventTarget = document.activeElement ?? window
  at.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, composed: true, cancelable: true, ...modifiers }))
}

/** Type `term` into the command palette's search, and wait for what it finds. */
async function search(term: string) {
  const input = document.querySelector<HTMLInputElement>('[data-testid="palette"] input')!
  input.value = term
  input.dispatchEvent(new Event('input', { bubbles: true }))
  // Fuse answers a frame later, its results throttled to one a frame.
  await new Promise(resolve => setTimeout(resolve, 40))
  await flushPromises()
}

// Neither is drawn until it opens, so the pages above draw neither: their
// icons are Nuxt UI's (its search, close, back and an entry's chevron) and
// the entries' own, which no page draws.
describe('the command palette and the sheet of shortcuts', () => {
  it('draw every icon from the bundle, an offer\'s projects, a link and what holds an install back among them, and add no style element', async () => {
    const cell = { project: 'web', reference: 'skill:format', origin: 'https://github.com/acme/skills', wanted: true }
    const why = { code: 'foreign', message: '`format` in web is somebody\'s own, at .claude/skills/format' }
    const held: Answered = { query: { type: 'plan', selection: [cell] }, answer: { tasks: [], conflicts: [], locked: [{ cell, why }] } }
    const { transport } = commanding(fixture('assigning'), applying(), [held])
    const { page } = await render('/', transport)

    press('k', { ctrlKey: true })
    await vi.waitFor(() => expect(document.querySelector('[data-testid="palette"] [role="option"]')).not.toBeNull())
    expect(document.querySelector('[data-testid="palette"] [data-slot="itemTrailingIcon"]'), 'an offer, which lists projects').not.toBeNull()
    drawsEveryIcon(page)

    await search('install format')
    expect(document.querySelector('[data-testid="palette"]')!.textContent).toContain('Link install format')
    drawsEveryIcon(page)

    press('Enter')
    await vi.waitFor(() => expect(document.querySelector('[data-testid="palette"] [data-slot="back"]')).not.toBeNull())
    drawsEveryIcon(page)

    await search('web')
    press('Enter')
    await vi.waitFor(() => expect(document.querySelector('li[data-state="open"]')?.textContent).toContain(why.code))
    drawsEveryIcon(page)

    press('?', { shiftKey: true })
    await vi.waitFor(() => expect(document.querySelector('[data-testid="shortcuts"]')).not.toBeNull())
    drawsEveryIcon(page)

    expect(styles).toEqual([])
  })
})
