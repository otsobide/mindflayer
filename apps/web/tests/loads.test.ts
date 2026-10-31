import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import { assignLink } from '@/routes'
import { useShell } from '@/stores/shell'
import { commanding, fixture, loadings, render, texts, unmount, type Commanded } from './panel'

afterEach(unmount)

const [KIT, TEAM, ITSELF] = loadings() as [Commanded, Commanded, Commanded]

/** What the dialog shows: each line, and each caution with its words. */
function shown() {
  return {
    lines: [...document.body.querySelectorAll('[data-testid="command"]')].map(each => each.textContent),
    cautions: [...document.body.querySelectorAll('[data-testid="caution"]')].map(each => each.textContent?.replace(/\s+/g, ' ').trim())
  }
}

/** Run what the dialog shows, and wait for it. */
async function run() {
  document.body.querySelector<HTMLElement>('[data-testid="run"]')!.click()
  await flushPromises()
}

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

  it('says the default remembers none while it is not made, never that no workspace is, and the command that makes it', async () => {
    const snapshot = fixture('unmade')

    const { page } = await render('/loads', snapshot)

    expect(snapshot.known).toEqual([])
    expect(snapshot.workspace).toMatchObject({ made: false, reason: 'home' })
    expect(page.get('[data-testid="none-known"]').text()).toBe(
      'The default workspace is not made yet, so it remembers none: load one made before it by its path below.'
    )
    expect(texts(page.get('[data-testid="no-known"]'), 'code')).toEqual([`$ flayer init ${snapshot.workspace.path}`])
    expect(page.find('[data-testid="load-form"]').exists()).toBe(true)
  })
})

describe('each change the page of loads sends', () => {
  it('offers to load each workspace this machine knows only where core says this one can load it', async () => {
    const snapshot = fixture('assigning')

    const { page } = await render('/loads', snapshot)

    const rows = page.findAll('[data-testid="known-workspace"]')
    expect(rows.map(row => row.find('[data-testid="load-known"]').exists())).toEqual(snapshot.known.map(known => known.loadable))
    expect(page.findAll('[data-testid="load-known"]').map(button => button.attributes('aria-label'))).toEqual(['Load you', 'Load kit'])
  })

  it('loads one by the path the snapshot lists it by: the TUI\'s task for it typed, shown before it runs', async () => {
    const { sent, transport } = commanding(fixture('assigning'), loadings())
    const { page } = await render('/loads', transport)

    await page.get('[data-testid="load-known"][aria-label="Load kit"]').trigger('click')
    await flushPromises()

    expect(sent).toEqual([{ task: { type: 'load', path: '/home/you/kit' }, mode: 'preview' }])
    expect(KIT.task).toEqual({ type: 'load', path: '/home/you/kit' })
    expect(shown()).toEqual({ lines: KIT.preview.lines.map(line => `$ ${line}`), cautions: [] })
    expect(document.body.querySelector('[data-testid="run"]')?.textContent?.trim()).toBe('Run')

    await run()

    expect(sent.filter(each => each.mode === 'run')).toEqual([{ task: KIT.task, mode: 'run' }])
  })

  it('unloads one by the entry its marker spells it by, asked first, as the TUI asks', async () => {
    const snapshot = fixture('assigning')
    const { sent, transport } = commanding(snapshot, loadings())
    const { page } = await render('/loads', transport)

    const unload = page.get('[data-testid="load"] [data-testid="unload"]')
    expect(unload.attributes('aria-label')).toBe('Unload team')
    await unload.trigger('click')
    await flushPromises()

    expect(sent).toEqual([{ task: { type: 'unload', entry: snapshot.loaded[0]!.entry }, mode: 'preview' }])
    expect(TEAM.task).toEqual({ type: 'unload', entry: '../team' })
    expect(shown()).toEqual({
      lines: TEAM.preview.lines.map(line => `$ ${line}`),
      cautions: TEAM.preview.confirm.map(each => `${each.caution}: ${each.message}`)
    })
    expect(document.activeElement?.getAttribute('data-testid')).toBe('cancel')

    await run()

    expect(sent.filter(each => each.mode === 'run')).toEqual([{ task: TEAM.task, mode: 'run' }])
  })

  it('offers to unload one that does not open, the one most worth letting go', async () => {
    const snapshot = fixture('workspace')

    const { page } = await render('/loads', snapshot)

    expect(snapshot.loaded[0]!.error).not.toBeNull()
    expect(page.get('[data-testid="load"] [data-testid="unload"]').attributes('aria-label')).toBe(`Unload ${snapshot.loaded[0]!.entry}`)
  })

  it('loads a path as typed, for the server to hold to the path policy and the line to refuse, saying why by its code', async () => {
    const { sent, transport } = commanding(fixture('assigning'), loadings())
    const { page } = await render('/loads', transport)
    const path = ITSELF.task.type === 'load' ? ITSELF.task.path : ''

    await page.get('input[data-testid="load-path"]').setValue(path)
    await page.get('[data-testid="load-form"]').trigger('submit')
    await flushPromises()

    expect(sent).toEqual([{ task: { type: 'load', path }, mode: 'preview' }])
    expect(shown().lines).toEqual(ITSELF.preview.lines.map(line => `$ ${line}`))

    await run()

    expect(sent.filter(each => each.mode === 'run')).toEqual([{ task: ITSELF.task, mode: 'run' }])
    await vi.waitFor(() => expect(useShell().warningsOpen).toBe(true))
    const error = ITSELF.ran.outcome.runs[0]!.error!
    expect(error.code).toBe('loads_itself')
    expect(document.body.querySelector('[data-testid="said"]')?.textContent).toContain(error.code)
    // Kept, to be corrected: the load did not go through.
    expect((page.get('input[data-testid="load-path"]').element as HTMLInputElement).value).toBe(path)
  })

  it('loads a path typed with white space around it without it, the task the TUI\'s form sends, and nothing for white space alone', async () => {
    const { sent, transport } = commanding(fixture('assigning'), loadings())
    const { page } = await render('/loads', transport)
    const path = KIT.task.type === 'load' ? KIT.task.path : ''

    await page.get('input[data-testid="load-path"]').setValue(' \t\u00a0')
    await page.get('[data-testid="load-form"]').trigger('submit')
    await flushPromises()

    expect(page.get('[data-testid="load-typed"]').attributes('disabled')).toBeDefined()
    expect(sent).toEqual([])

    // U+0085 is white space to Rust's `str::trim`, which the TUI's form
    // trims with, and not to JavaScript's `trim`.
    await page.get('input[data-testid="load-path"]').setValue(`  ${path}\u0085 `)
    await page.get('[data-testid="load-form"]').trigger('submit')
    await flushPromises()

    expect(sent).toEqual([{ task: KIT.task, mode: 'preview' }])
    expect(shown().lines).toEqual(KIT.preview.lines.map(line => `$ ${line}`))
  })

  it('empties the field once a typed path was loaded', async () => {
    const { transport } = commanding(fixture('assigning'), loadings())
    const { page } = await render('/loads', transport)

    await page.get('input[data-testid="load-path"]').setValue('/home/you/kit')
    await page.get('[data-testid="load-form"]').trigger('submit')
    await flushPromises()
    await run()

    expect((page.get('input[data-testid="load-path"]').element as HTMLInputElement).value).toBe('')
  })
})
