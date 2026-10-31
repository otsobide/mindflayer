import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import type { Snapshot } from '@/api/generated/Snapshot'
import { VIEWS } from '@/components/AppSidebar.vue'
import { useShell } from '@/stores/shell'
import { answering, answers, commanding, drafts, fixture, render, unmount } from './panel'

afterEach(() => {
  unmount()
  vi.restoreAllMocks()
})

/** Press `key` where the focus is, or on the window when nothing holds it, as a keyboard would. */
function press(key: string, modifiers: KeyboardEventInit = {}, at: EventTarget = document.activeElement ?? window) {
  at.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, composed: true, cancelable: true, ...modifiers }))
}

/** Wait for what a key or a choice set off: the palette drawn, gone, or a page opened. */
async function settled() {
  await flushPromises()
  await new Promise(resolve => setTimeout(resolve, 0))
  await flushPromises()
}

/** The palette, while it is open. */
function palette(): HTMLElement | null {
  return document.querySelector<HTMLElement>('[role="dialog"]:has([data-testid="palette"])')
}

/** Type `term` into the palette's search, as a person types it. */
async function search(term: string) {
  const input = palette()!.querySelector<HTMLInputElement>('input')!
  input.value = term
  input.dispatchEvent(new Event('input', { bubbles: true }))
  // Fuse answers a frame later, its results throttled to one a frame.
  await new Promise(resolve => setTimeout(resolve, 40))
  await flushPromises()
}

/** Each group the palette lists, by its heading, with the label of each entry in it. */
function listed(): { group: string, entries: string[] }[] {
  return [...palette()!.querySelectorAll('[role="group"]')].map(group => ({
    group: document.getElementById(group.getAttribute('aria-labelledby')!)!.textContent!.trim(),
    entries: [...group.querySelectorAll('[role="option"]')].map(entry => [...entry.querySelectorAll('[data-slot="itemLabelBase"], [data-slot="itemLabelSuffix"]')]
      .map(part => part.textContent!.trim())
      .join(' '))
  }))
}

/** Every entry's label, in the order listed. */
function entries(): string[] {
  return listed().flatMap(group => group.entries)
}

/** Open the palette as a keyboard does, over whatever has the focus. */
async function opened() {
  press('k', { ctrlKey: true })
  await settled()
  expect(palette(), 'the palette, opened by Ctrl+K').not.toBeNull()
}

describe('the command palette', () => {
  it('opens on Ctrl+K over any page, its search focused, and gives the focus back where it was on Escape', async () => {
    await render('/projects', fixture('workspace'))
    const before = document.querySelector<HTMLElement>('[data-testid="sidebar"] nav a[href="/assign"]')!
    before.focus()

    await opened()
    expect(useShell().paletteOpen).toBe(true)
    expect(document.activeElement).toBe(palette()!.querySelector('input'))

    press('Escape')
    await vi.waitFor(() => expect(palette()).toBeNull())
    await settled()
    expect(useShell().paletteOpen).toBe(false)
    expect(document.activeElement).toBe(before)
  })

  it('opens on Cmd+K on a Mac, and on Ctrl+K there too', async () => {
    vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15')
    await render('/', fixture('workspace'))

    press('k', { metaKey: true })
    await settled()
    expect(palette()).not.toBeNull()

    press('Escape')
    await vi.waitFor(() => expect(palette()).toBeNull())
    press('k', { ctrlKey: true })
    await settled()
    expect(palette()).not.toBeNull()
  })

  it('opens from the sidebar, which says its keys', async () => {
    const { page } = await render('/', fixture('workspace'))
    const button = page.get('[data-testid="open-palette"]')

    expect(button.attributes('aria-keyshortcuts')).toBe('Control+K Meta+K')
    await button.trigger('click')
    await settled()

    expect(palette()).not.toBeNull()
  })

  it('never opens while a field or the editor has the focus, whose keys they stay', async () => {
    const { page } = await render('/projects', fixture('workspace'))
    page.get<HTMLInputElement>('input[data-testid="filter-text"]').element.focus()
    press('k', { ctrlKey: true })
    await settled()
    expect(palette()).toBeNull()
    unmount()

    const editing = await render('/edit?kind=skill&name=review', answering(fixture('workspace'), [...answers(), ...drafts()]).transport)
    const content = await vi.waitFor(() => {
      const found = editing.page.get('[data-testid="editor"]').element.shadowRoot?.querySelector<HTMLElement>('.cm-content')
      expect(found).toBeTruthy()
      return found!
    })
    content.focus()
    press('k', { ctrlKey: true }, content)
    await settled()
    expect(palette()).toBeNull()
  })

  it('never opens while a task is shown, whose question is answered first', async () => {
    const { transport } = commanding()
    const { page } = await render('/project?entry=gone', transport)
    await page.get('[data-testid="unlink"]').trigger('click')
    await settled()
    expect(document.querySelector('[data-testid="preview"]')).not.toBeNull()

    press('k', { ctrlKey: true })
    await settled()

    expect(useShell().paletteOpen).toBe(false)
    expect(palette()).toBeNull()
  })

  it('lists every page, then each project, the library\'s own, the shelf and the sources, each under its heading', async () => {
    const snapshot = fixture('workspace')
    await render('/', snapshot)
    await opened()

    expect(listed()).toEqual([
      { group: 'Pages', entries: VIEWS.map(view => view.label) },
      { group: 'Projects', entries: snapshot.projects.map(project => `${project.name ?? project.entry} ${project.health}`) },
      { group: 'Library', entries: snapshot.library.artifacts.map(held => `${held.name} ${held.kind}`) },
      { group: 'Shelf', entries: snapshot.shelf.map(shelved => `${shelved.name} ${shelved.kind}`) },
      { group: 'Sources', entries: snapshot.sources.map(source => `${source.origin} ${source.kind}`) }
    ])
    expect(palette()!.querySelector('[role="listbox"]')!.getAttribute('aria-label')).toBeTruthy()
    expect(palette()!.querySelector('input')!.getAttribute('aria-label')).toBe('Search the panel')
    expect(palette()!.getAttribute('aria-labelledby')).toBeTruthy()
  })

  it('searches fuzzily, every word narrowing what the others found, the group of the best match first', async () => {
    await render('/', fixture('workspace'))
    await opened()

    await search('revew')
    expect(listed()[0]).toEqual({ group: 'Library', entries: ['review skill'] })

    await search('projets')
    expect(entries()).toContain('Projects')

    // Near enough to find `comments` too, but `commits` first; and every
    // skill, which the first word finds nothing of, left out.
    await search('rule commits')
    expect(entries()).toEqual(['git/commits rule', 'style/comments rule'])

    // The source, whose origin it is all of, before what it put on the shelf.
    await search('acme')
    expect(listed().map(group => group.group)).toEqual(['Sources', 'Shelf'])

    await search('nothing here at all')
    expect(entries()).toEqual([])
  })

  it('goes where a link on a page would, once it has closed', async () => {
    const { router } = await render('/', fixture('workspace'))

    for (const [term, path] of [
      ['docs broken', '/project?entry=docs'],
      ['review', '/artifact?kind=skill&name=review'],
      ['triage', '/shelf?kind=skill&q=triage'],
      ['acme git', '/sources'],
      ['assign', '/assign']
    ]) {
      await opened()
      await search(term!)
      press('Enter')
      await vi.waitFor(() => expect(router.currentRoute.value.fullPath).toBe(path))
      expect(palette()).toBeNull()
      await settled()
    }
  })

  it('draws what it lists as text, never as HTML', async () => {
    const snapshot: Snapshot = fixture('workspace')
    snapshot.library.artifacts[0]!.summary = '<img src="x" alt="planted"> a <b>bold</b> summary'
    await render('/', snapshot)
    await opened()

    await search('bold')

    const found = palette()!.querySelector('[role="option"]')!
    expect(found.textContent).toContain('<img src="x" alt="planted"> a <b>bold</b> summary')
    expect(found.querySelector('img, b, mark')).toBeNull()
  })

  it('asks the server for nothing, the snapshot read from what is kept while the server watches', async () => {
    const { queries, tell, transport } = commanding(fixture('workspace'))
    await render('/', transport)
    tell({ type: 'watching' })
    await flushPromises()
    const asked = queries.length

    await opened()
    await search('api')

    expect(queries.slice(asked)).toEqual([])
  })
})
