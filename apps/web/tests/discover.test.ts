import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import type { Query } from '@/api/transport'
import type { Scanned } from '@/api/generated/Scanned'
import { answering, commanded, commanding, fixture, render, scans, texts, unmount } from './panel'

/*
 * The page of what could be linked, over the scan a test of the CLI wrote
 * (tests/fixtures/scan.json): the workspace's folder, with two projects
 * nobody linked, two repositories that are not projects yet, a project
 * moved from the entry pointing at it, and the entry `gone`, whose id the
 * ledger never learned. Drawn beside the snapshot of the workspace it
 * scanned (the `candidates` fixture). What it sends is held to the tasks
 * the CLI's suite previewed and ran (tests/fixtures/commands.json).
 */

afterEach(unmount)

/** The scan the fixture answers, as the page asks it. */
const [SCAN] = scans() as [{ query: Query, answer: Scanned }]
const ROOT = '/home/you/Projects'

/** The marks, the relink: what the page sends, as the CLI's suite sent them. */
const MARKS = commanded().find(each => each.task.type === 'marks')!
const RELINK = commanded().find(each => each.task.type === 'relink')!

/** Each scan the page asked for. */
function scansAsked(queries: Query[]): Query[] {
  return queries.filter(query => query.type === 'scan')
}

/** The element `selector` finds anywhere on the page, the dialogs teleported to its body among it. */
function find(selector: string): HTMLElement | null {
  return document.body.querySelector<HTMLElement>(selector)
}

/** The checkbox marking the suggestion found at `route`. */
function checkbox(route: string): HTMLElement {
  const row = [...document.body.querySelectorAll<HTMLElement>('[data-testid="suggestion"]')]
    .find(each => each.querySelector('label')?.textContent?.trim() === route)
  const box = row?.querySelector<HTMLElement>('[role="checkbox"]')
  if (!box) {
    throw new Error(`no suggestion is ${route}`)
  }
  return box
}

describe('the page of what could be linked', () => {
  it('scans nothing until asked, and starts from the workspace\'s own folder', async () => {
    const { queries, transport } = answering(fixture('candidates'))

    const { page } = await render('/discover', transport)

    expect(scansAsked(queries)).toEqual([])
    expect((page.get('textarea[data-testid="scan-roots"]').element as HTMLTextAreaElement).value).toBe(ROOT)
    expect(page.find('[data-testid="not-asked"]').exists()).toBe(true)
  })

  it('scans the folders typed, one a line, as typed, and keeps them in the address', async () => {
    const { queries, transport } = answering(fixture('candidates'))
    const { page, router } = await render('/discover', transport)

    await page.get('textarea[data-testid="scan-roots"]').setValue(`${ROOT}\n\n/srv/git `)
    await page.get('[data-testid="scan-form"]').trigger('submit')
    await flushPromises()

    expect(scansAsked(queries)).toEqual([{ type: 'scan', roots: [ROOT, '/srv/git '], depth: null }])
    expect(router.currentRoute.value.query.root).toEqual([ROOT, '/srv/git '])
  })

  it('opens with the scan the address names, as after a reload, and lists what it found in its order', async () => {
    const { queries, transport } = answering(fixture('candidates'))

    const { page } = await render(`/discover?root=${encodeURIComponent(ROOT)}`, transport)

    expect(scansAsked(queries)).toEqual([SCAN.query])
    const offered = SCAN.answer.found.filter(suggestion => suggestion.offers !== null)
    expect(texts(page, '[data-testid="suggestion"] label')).toEqual(offered.map(suggestion => suggestion.found.route))
    expect(texts(page, '[data-testid="suggestion"] [data-testid="reason"]')).toEqual(offered.map(suggestion => suggestion.found.reason))
    expect(texts(page, '[data-testid="suggestion"] [data-testid="why"]')).toEqual(offered.map(suggestion => suggestion.why))
    expect(texts(page, '[data-testid="suggestion"] label')).toEqual(['blog', 'drafts', 'notes', 'scratch'])
    expect(texts(page, '[data-testid="moved-row"] [data-testid="why"]')).toEqual(['moved: old -> archive/old'])
    expect(texts(page, '[data-testid="linked-route"]')).toEqual(['api', 'docs', 'legacy', 'web'])
    expect(texts(page, '[data-testid="untraced"] code')).toEqual([`$ flayer -W ${ROOT} relink gone <PATH>`])
    expect(page.get('[data-testid="scanned"]').text()).toContain(`${ROOT} to depth 3`)
  })

  it('walks the disk again when Scan is pressed, never drawing what an earlier visit was answered with', async () => {
    const { queries, transport } = answering(fixture('candidates'))
    const { page, router } = await render(`/discover?root=${encodeURIComponent(ROOT)}`, transport)

    // Away, then back by the sidebar's link, which names no folder: the
    // form starts from the workspace's own folder again, the one scanned.
    await router.push('/projects')
    await router.push('/discover')
    await flushPromises()
    expect(page.find('[data-testid="not-asked"]').exists()).toBe(true)
    await page.get('[data-testid="scan-form"]').trigger('submit')
    await flushPromises()

    expect(scansAsked(queries)).toEqual([SCAN.query, SCAN.query])
    expect(router.currentRoute.value.query.root).toEqual([ROOT])
  })

  it('walks the disk again for folders scanned earlier in the same visit', async () => {
    const { queries, transport } = answering(fixture('candidates'))
    const { page } = await render(`/discover?root=${encodeURIComponent(ROOT)}`, transport)

    for (const typed of ['/srv/git', ROOT]) {
      await page.get('textarea[data-testid="scan-roots"]').setValue(typed)
      await page.get('[data-testid="scan-form"]').trigger('submit')
      await flushPromises()
    }

    expect(scansAsked(queries).map(query => query.type === 'scan' && query.roots)).toEqual([[ROOT], ['/srv/git'], [ROOT]])
    expect(page.find('[data-testid="scanned"]').exists()).toBe(true)
  })

  it('takes a depth emptied, or one the address cannot read as a whole number, for core\'s own', async () => {
    const { queries, transport } = answering(fixture('candidates'))
    const { page, router } = await render(`/discover?root=${encodeURIComponent(ROOT)}&depth=abc`, transport)
    const input = page.get('[data-testid="scan-depth"] input, input[data-testid="scan-depth"]')

    for (const typed of ['2', '']) {
      await input.setValue(typed)
      await input.trigger('blur')
      await page.get('[data-testid="scan-form"]').trigger('submit')
      await flushPromises()
    }

    expect(scansAsked(queries).map(query => query.type === 'scan' && query.depth)).toEqual([null, 2, null])
    expect(router.currentRoute.value.query.depth).toBeUndefined()
  })

  it('is never scanned again by what the server streams, only by asking', async () => {
    const { queries, tell, transport } = commanding(fixture('candidates'))
    const { page } = await render(`/discover?root=${encodeURIComponent(ROOT)}`, transport)

    tell({ type: 'changed', scopes: [{ type: 'workspace' }] })
    tell({ type: 'resync', switches: 0 })
    await flushPromises()
    expect(scansAsked(queries)).toHaveLength(1)

    await page.get('[data-testid="scan-form"]').trigger('submit')
    await flushPromises()
    expect(scansAsked(queries)).toHaveLength(2)
  })

  it('says why the server refused a scan, with its code', async () => {
    const { transport } = answering(fixture('candidates'), [])

    const { page } = await render('/discover?root=Projects', transport)

    await vi.waitFor(() => expect(page.find('[data-testid="scan-refused"]').exists()).toBe(true))
    expect(page.get('[data-testid="scan-refused"]').text()).toContain('not_found')
  })

  it('says what the scan could not read, with each code, as flayer scan warns of it', async () => {
    const answered = scans()
    const answer = answered[0]!.answer as Scanned
    answer.warnings = [{ text: '/home/you/wrok: cannot be listed: No such file or directory (os error 2)', code: 'scan_folder_unlisted' }]
    answer.found = []
    const { transport } = answering(fixture('candidates'), answered)

    const { page } = await render(`/discover?root=${encodeURIComponent(ROOT)}`, transport)

    expect(texts(page, '[data-testid="scan-warning"]')).toEqual([
      'scan_folder_unlisted: /home/you/wrok: cannot be listed: No such file or directory (os error 2)'
    ])
    expect(page.find('[data-testid="nothing-found"]').exists()).toBe(true)
  })
})

describe('linking what a scan found', () => {
  it('sends the marks the minds screen builds its batch from, shows the lines the server previewed, and scans again after', async () => {
    const { queries, sent, transport } = commanding(fixture('candidates'))
    const { page } = await render(`/discover?root=${encodeURIComponent(ROOT)}`, transport)
    expect(page.get('[data-testid="link-marked"]').attributes('disabled')).toBeDefined()

    checkbox('notes').click()
    checkbox('drafts').click()
    await flushPromises()

    // A repository is made a project first, which is said before anything is asked.
    expect(page.get('[data-testid="writes-into"]').text()).toContain('notes: not a mind project yet')
    await page.get('[data-testid="link-marked"]').trigger('click')
    await flushPromises()

    // In the scan's order, whatever order they were marked in.
    expect(sent).toEqual([{ task: MARKS.task, mode: 'preview' }])
    expect([...document.body.querySelectorAll('[data-testid="command"]')].map(each => each.textContent)).toEqual(MARKS.preview.lines.map(line => `$ ${line}`))
    find('[data-testid="run"]')!.click()
    await flushPromises()

    expect(sent).toEqual([{ task: MARKS.task, mode: 'preview' }, { task: MARKS.task, mode: 'run' }])
    await vi.waitFor(() => expect(scansAsked(queries)).toHaveLength(2))
  })

  it('relinks a project the scan found moved, by the entry it left, to where it is now', async () => {
    const { sent, transport } = commanding(fixture('candidates'))
    const { page } = await render(`/discover?root=${encodeURIComponent(ROOT)}`, transport)

    await page.get('[data-testid="relink"]').trigger('click')
    await flushPromises()

    expect(sent).toEqual([{ task: RELINK.task, mode: 'preview' }])
    expect([...document.body.querySelectorAll('[data-testid="command"]')].map(each => each.textContent)).toEqual(RELINK.preview.lines.map(line => `$ ${line}`))
    expect(document.body.querySelectorAll('[data-testid="caution"]')).toHaveLength(0)
  })

  it('offers nothing to mark for what is linked, or for a project found moved', async () => {
    const { page } = await render(`/discover?root=${encodeURIComponent(ROOT)}`, fixture('candidates'))

    const marked = texts(page, '[data-testid="suggestion"] label')
    for (const route of ['api', 'docs', 'legacy', 'web', 'archive/old']) {
      expect(marked).not.toContain(route)
    }
  })
})

describe('the frame', () => {
  it('lights Discover in the sidebar, and the projects page links to it', async () => {
    const { page, router } = await render('/discover', fixture('candidates'))

    expect(page.get('[data-testid="sidebar"] nav a[aria-current="page"]').text()).toBe('Discover')

    await router.push('/projects')
    await flushPromises()
    expect(page.get('[data-testid="discover"]').attributes('href')).toBe('/discover')
  })
})
