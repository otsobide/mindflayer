import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import type { Folders } from '@/api/generated/Folders'
import { TransportError, type Query, type Transport } from '@/api/transport'
import { commanded, commanding, fixture, render, unmount, walks, type Answered } from './panel'

/*
 * The explorer, opened from the Link form on the projects page, over what
 * the CLI's suite was answered with as it walked the folders of the
 * candidates fixture (tests/fixtures/folders.json): the workspace at
 * /home/you/Projects, in the home /home/you. What the Link form then sends
 * is held to the link the CLI's suite previewed and ran
 * (tests/fixtures/commands.json).
 */

afterEach(unmount)

const ROOT = '/home/you/Projects'
const WALK = walks()

/** `query`, a flat one, as text whatever order its keys were written in. */
function keyed(query: Query): string {
  return JSON.stringify(Object.entries(query).sort(([a], [b]) => a.localeCompare(b)))
}

/** What the fixture answers `query` with. */
function answer(query: Query): Folders {
  const found = WALK.walked.find(each => keyed(each.query) === keyed(query))
  if (!found) {
    throw new Error(`the fixture answers no ${JSON.stringify(query)}`)
  }
  return found.answer as Folders
}

/** The listing of `path`, none for the workspace's root, hidden folders as `hidden` says. */
function listing(path: string | null, hidden = false): Query {
  return { type: 'folders', path, hidden }
}

/**
 * A transport answering as the candidates fixture's server does: the
 * snapshot, the explorer's walk, and the link the CLI's suite ran; the
 * path the fixture holds refused, refused with its code. Each query and
 * each task written down.
 */
function exploring(walked: Answered[] = WALK.walked) {
  const { queries, sent, tell, transport } = commanding(fixture('candidates'), commanded(), walked)
  const refusing = {
    ...transport,
    query(query: Query) {
      if (keyed(query) === keyed(WALK.refused.query)) {
        queries.push(query)
        return Promise.reject(new TransportError(422, WALK.refused.error, WALK.refused.code))
      }
      return transport.query(query)
    }
  } as unknown as Transport
  return { queries, sent, tell, transport: refusing }
}

/** The element `selector` finds anywhere, the explorer teleported to the body among it. */
function find(selector: string): HTMLElement | null {
  return document.body.querySelector<HTMLElement>(selector)
}

/** The text of every element `selector` finds anywhere, trimmed. */
function all(selector: string): string[] {
  return [...document.body.querySelectorAll<HTMLElement>(selector)].map(each => (each.textContent ?? '').replace(/\s+/g, ' ').trim())
}

/** Each listing the panel asked for. */
function listings(queries: Query[]): Query[] {
  return queries.filter(query => query.type === 'folders')
}

/** Press what `selector` finds, and wait for what it asks for to be drawn. */
async function press(selector: string) {
  const found = find(selector)
  if (!found) {
    throw new Error(`nothing is ${selector}`)
  }
  found.click()
  await flushPromises()
}

/** The row of the folder `name` in the listing shown. */
function row(name: string): HTMLElement {
  const found = [...document.body.querySelectorAll<HTMLElement>('[data-testid="folder"]')]
    .find(each => each.querySelector('[data-testid="folder-open"]')?.textContent?.trim() === name)
  if (!found) {
    throw new Error(`no folder is listed as ${name}`)
  }
  return found
}

/** Open the folder `name` from its row. */
async function open(name: string) {
  row(name).querySelector<HTMLElement>('[data-testid="folder-open"]')!.click()
  await flushPromises()
}

/** Type `text` into the input `selector` finds, as a person would. */
async function type(selector: string, text: string) {
  const input = find(selector) as HTMLInputElement | HTMLTextAreaElement
  input.value = text
  input.dispatchEvent(new Event('input'))
  await flushPromises()
}

/** Wait for the explorer to show a listing. */
async function listed() {
  await vi.waitFor(() => expect(find('[data-testid="folder-here"]')).not.toBeNull())
}

describe('the explorer, from the Link form', () => {
  it('opens at the workspace\'s root while the field holds nothing, saying what each folder is in words', async () => {
    const { queries, transport } = exploring()
    await render('/projects', transport)

    await press('[data-testid="link-browse"]')
    await listed()

    expect(listings(queries)).toEqual([listing(null)])
    const root = answer(listing(null))
    expect(all('[data-testid="folder"] [data-testid="folder-open"]')).toEqual(root.folders.map(folder => folder.name))
    expect(all('[data-testid="folder"] [data-testid="folder-is"]')).toEqual([
      'mind project', 'folder', 'mind project', 'mind project', 'mind project', 'mind project', 'git repository', 'git repository', 'mind project'
    ])
    const linked = [...document.body.querySelectorAll('[data-testid="folder"]')]
      .filter(each => each.querySelector('[data-testid="folder-linked"]'))
      .map(each => each.querySelector('[data-testid="folder-open"]')!.textContent!.trim())
    expect(linked).toEqual(root.folders.filter(folder => folder.linked).map(folder => folder.name))
    expect(linked).toEqual(['api', 'docs', 'legacy', 'web'])
    expect(all('[data-testid="folder-here"] [data-testid="folder-is"]')).toEqual(['flayer workspace'])
    expect(all('[data-testid="crumb"]')).toEqual(['/', 'home', 'you'])
    expect(all('[data-testid="crumb-here"]')).toEqual(['Projects'])
    expect((find('[data-testid="folder-path"] input, input[data-testid="folder-path"]') as HTMLInputElement).value).toBe(ROOT)
    expect(find('[data-testid="folder-home"]')).not.toBeNull()
  })

  it('walks down into a folder, and back up by its trail', async () => {
    const { queries, transport } = exploring()
    await render('/projects', transport)
    await press('[data-testid="link-browse"]')
    await listed()

    await open('archive')
    await vi.waitFor(() => expect(all('[data-testid="crumb-here"]')).toEqual(['archive']))
    expect(all('[data-testid="folder"] [data-testid="folder-open"]')).toEqual(['old'])
    expect(all('[data-testid="folder"] [data-testid="folder-is"]')).toEqual(['mind project'])

    const up = [...document.body.querySelectorAll<HTMLElement>('[data-testid="crumb"]')].find(each => each.textContent?.trim() === 'you')!
    up.click()
    await flushPromises()
    await vi.waitFor(() => expect(all('[data-testid="crumb-here"]')).toEqual(['you']))
    expect(all('[data-testid="folder"] [data-testid="folder-is"]')).toEqual(['flayer workspace'])

    expect(listings(queries)).toEqual([listing(null), listing(`${ROOT}/archive`), listing('/home/you')])
  })

  it('goes back to the home and to the workspace\'s root from anywhere', async () => {
    const { queries, transport } = exploring()
    await render('/projects', transport)
    await press('[data-testid="link-browse"]')
    await listed()

    await press('[data-testid="folder-home"]')
    await vi.waitFor(() => expect(all('[data-testid="crumb-here"]')).toEqual(['you']))
    await press('[data-testid="folder-workspace"]')
    await vi.waitFor(() => expect(all('[data-testid="crumb-here"]')).toEqual(['Projects']))

    expect(listings(queries)).toEqual([listing(null), listing('/home/you'), listing(null)])
  })

  it('shows hidden folders only when asked for, saying how many there are', async () => {
    const { queries, transport } = exploring()
    await render('/projects', transport)
    await press('[data-testid="link-browse"]')
    await listed()
    await open('notes')
    await vi.waitFor(() => expect(all('[data-testid="crumb-here"]')).toEqual(['notes']))

    expect(find('[data-testid="folder"]')).toBeNull()
    expect(all('[data-testid="folder-empty"]')).toEqual(['No folders in it, but 1 hidden.'])
    expect(find('[data-testid="folder-picker"]')!.textContent).toContain('Hidden folders (1)')

    await press('[data-testid="folder-hidden"] [role="switch"], [role="switch"][data-testid="folder-hidden"]')
    await vi.waitFor(() => expect(all('[data-testid="folder"] [data-testid="folder-open"]')).toEqual(['.git']))

    expect(listings(queries).slice(-2)).toEqual([listing(`${ROOT}/notes`), listing(`${ROOT}/notes`, true)])
  })

  it('puts the folder chosen in the field, which the link previews as one typed', async () => {
    const [link] = commanded()
    const { sent, transport } = exploring()
    const { page } = await render('/projects', transport)
    await press('[data-testid="link-browse"]')
    await listed()

    row('blog').querySelector<HTMLElement>('[data-testid="folder-choose"]')!.click()
    await flushPromises()

    await vi.waitFor(() => expect(find('[data-testid="folder-picker"]')).toBeNull())
    expect(link!.task).toEqual({ type: 'link', path: `${ROOT}/blog` })
    expect((page.get('input[data-testid="link-path"]').element as HTMLInputElement).value).toBe(`${ROOT}/blog`)
    expect(sent).toEqual([])

    await page.get('[data-testid="link-form"]').trigger('submit')
    await flushPromises()

    expect(sent).toEqual([{ task: link!.task, mode: 'preview' }])
    expect(all('[data-testid="command"]')).toEqual(link!.preview.lines.map(line => `$ ${line}`))
  })

  it('opens at the folder the field holds, which can be chosen itself', async () => {
    const { queries, transport } = exploring()
    const { page } = await render('/projects', transport)
    await page.get('input[data-testid="link-path"]').setValue(`${ROOT}/archive`)

    await press('[data-testid="link-browse"]')
    await listed()

    expect(listings(queries)).toEqual([listing(`${ROOT}/archive`)])
    expect(all('[data-testid="folder-here"] [data-testid="folder-is"]')).toEqual(['folder'])
    await press('[data-testid="folder-choose-here"]')
    await vi.waitFor(() => expect(find('[data-testid="folder-picker"]')).toBeNull())
    expect((page.get('input[data-testid="link-path"]').element as HTMLInputElement).value).toBe(`${ROOT}/archive`)
  })

  it('sends a path typed as it is, says why it was refused with its code, and stays where it was', async () => {
    const { queries, transport } = exploring()
    await render('/projects', transport)
    await press('[data-testid="link-browse"]')
    await listed()

    await type('input[data-testid="folder-path"], [data-testid="folder-path"] input', 'Projects')
    await press('[data-testid="folder-go"]')

    expect(listings(queries).at(-1)).toEqual(WALK.refused.query)
    await vi.waitFor(() => expect(find('[data-testid="folder-refused"]')).not.toBeNull())
    expect(find('[data-testid="folder-refused"]')!.textContent).toContain(`(${WALK.refused.code})`)
    expect(WALK.refused.code).toBe('path_relative')
    expect(all('[data-testid="crumb-here"]')).toEqual(['Projects'])

    await open('archive')
    await vi.waitFor(() => expect(find('[data-testid="folder-refused"]')).toBeNull())
  })

  it('reads the folder again each time it opens, and never for what the server streams', async () => {
    const { queries, tell, transport } = exploring()
    await render('/projects', transport)
    await press('[data-testid="link-browse"]')
    await listed()

    tell({ type: 'changed', scopes: [{ type: 'workspace' }] })
    tell({ type: 'resync', switches: 0 })
    await flushPromises()
    expect(listings(queries)).toHaveLength(1)

    await press('[data-testid="folder-cancel"]')
    await vi.waitFor(() => expect(find('[data-testid="folder-picker"]')).toBeNull())
    await press('[data-testid="link-browse"]')
    await listed()

    expect(listings(queries)).toEqual([listing(null), listing(null)])
  })

  it('is worked from the keyboard: every control a button or a field, the path\'s in a form its button submits', async () => {
    const { queries, transport } = exploring()
    await render('/projects', transport)
    await press('[data-testid="link-browse"]')
    await listed()

    // A browser submits the form on Enter in its one field, which the
    // browser suite presses; happy-dom does not, so its button is pressed.
    const field = find('input[data-testid="folder-path"], [data-testid="folder-path"] input') as HTMLInputElement
    expect(field.form).toBe(find('[data-testid="folder-go"]')!.closest('form'))
    expect(find('[data-testid="folder-go"]')!.getAttribute('type')).toBe('submit')
    await type('input[data-testid="folder-path"], [data-testid="folder-path"] input', `${ROOT}/archive`)
    await press('[data-testid="folder-go"]')

    expect(listings(queries).at(-1)).toEqual(listing(`${ROOT}/archive`))
    for (const control of document.body.querySelectorAll('[data-testid="crumb"], [data-testid="folder-open"], [data-testid="folder-choose"], [data-testid="folder-home"], [data-testid="folder-workspace"], [data-testid="folder-choose-here"], [data-testid="folder-cancel"], [data-testid="folder-go"]')) {
      expect(control.tagName).toBe('BUTTON')
    }
    expect(document.body.querySelector('[data-testid="folder-hidden"] [role="switch"], [role="switch"][data-testid="folder-hidden"]')!.tagName).toBe('BUTTON')
  })

  it('marks a link to a folder, and says what it could not read with each code', async () => {
    const walked = walks().walked
    const root = walked[0]!.answer as Folders
    root.folders[0]!.symlink = true
    root.failures = [{ code: 'browse_name_unfit', message: `${ROOT}/� has a name that is not UTF-8, which no request can send back, so it is not listed` }]
    root.truncated = true
    const { transport } = exploring(walked)
    await render('/projects', transport)

    await press('[data-testid="link-browse"]')
    await listed()

    expect(row('api').querySelector('[data-testid="folder-symlink"]')!.textContent).toContain('symlink')
    expect(find('[data-testid="folder-symlink"]')).toBe(row('api').querySelector('[data-testid="folder-symlink"]'))
    expect(all('[data-testid="folder-failure"]')).toEqual([`browse_name_unfit: ${root.failures[0]!.message}`])
    expect(find('[data-testid="folder-truncated"]')).not.toBeNull()
  })
})
