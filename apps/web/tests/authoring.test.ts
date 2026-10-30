import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, type VueWrapper } from '@vue/test-utils'
import { EditorView } from '@codemirror/view'
import type { Contents } from '@/api/generated/Contents'
import type { Draft } from '@/api/generated/Draft'
import type { Judged } from '@/api/generated/Judged'
import { answers, authored, commanding, drafts, fixture, render, unmount, validated, type Answered, type Commanded } from './panel'

/** Every `<style>` element anything adds while a test runs, in the page or in a shadow root, which the CSP would refuse. */
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

/** What judging `draft` in the workspace's own is answered with, in tests/fixtures/drafts.json. */
function judged(draft: Partial<Draft>): Judged {
  const found = drafts().find(each => each.query.type === 'validate' && Object.entries(draft).every(([key, value]) => (each.query as { draft: Record<string, unknown> }).draft[key] === value))
  if (!found) {
    throw new Error(`no fixture judges ${JSON.stringify(draft)}`)
  }
  return found.answer as Judged
}

/** What the library's `review` is read as, its SKILL.md: the text the editor starts from. */
function review(): Contents {
  const found = answers().find(each => each.query.type === 'contents' && each.query.reference === 'skill:review' && each.query.file === 'SKILL.md')
  return found!.answer as Contents
}

/** The one change of tests/fixtures/authored.json that `type` names, the `nth` of them. */
function change(type: Commanded['task']['type'], nth = 0): Commanded {
  return authored().filter(each => each.task.type === type)[nth]!
}

/** The editor on the page, inside the shadow root it is drawn in. */
function editor(page: VueWrapper): { shadow: ShadowRoot, view: EditorView } {
  const shadow = page.get('[data-testid="editor"]').element.shadowRoot!
  const view = EditorView.findFromDOM(shadow.querySelector<HTMLElement>('.cm-editor')!)!
  return { shadow, view }
}

/** Type `text` over everything the editor holds, as somebody would, then wait for it to be judged. */
async function type(view: EditorView, text: string) {
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } })
  await new Promise(resolve => setTimeout(resolve, 300))
  await flushPromises()
}

/** The lines the dialog shows, as the preview gave them. */
function shown(): (string | null)[] {
  return [...document.body.querySelectorAll('[data-testid="command"]')].map(each => each.textContent)
}

/** The lines `flayer validate` said of `review` in the library, each issue's words alone. */
function saidOfReview(): { warning: boolean, message: string }[] {
  const lines = validated.said.split('\n')
  const start = lines.findIndex(line => line.startsWith('skill:review (workspace)'))
  const said = []
  for (const line of lines.slice(start + 1)) {
    if (!line.startsWith('  - ')) {
      break
    }
    const issue = line.slice('  - '.length)
    said.push(issue.startsWith('warning: ') ? { warning: true, message: issue.slice('warning: '.length) } : { warning: false, message: issue })
  }
  return said
}

const EDITOR = '/edit?kind=skill&name=review'

describe('the editor', () => {
  it('starts from the file as core read it, drawn in a shadow root its styles are adopted into, and adds no style element', async () => {
    const { page } = await render(EDITOR, commanding(fixture('workspace'), [], [...answers(), ...drafts()]).transport)

    const { shadow, view } = editor(page)
    expect(view.state.doc.toString()).toBe(review().text)
    // style-mod's sheet, constructed and adopted: never a <style> element,
    // which the CSP would refuse, in the page or in the shadow root.
    expect(shadow.adoptedStyleSheets.length).toBeGreaterThan(0)
    expect(shadow.adoptedStyleSheets[0]!.cssRules.length).toBeGreaterThan(0)
    expect(shadow.querySelectorAll('style')).toHaveLength(0)
    expect(styles).toEqual([])
    expect(shadow.querySelector('[role="textbox"]')!.getAttribute('aria-label')).toBe('The text of review\'s SKILL.md')
    expect(page.get('[data-testid="save"]').attributes('disabled')).toBeDefined()
    expect(page.find('[data-testid="unsaved"]').exists()).toBe(false)
    expect(page.get('[data-testid="verdict"]').text()).toContain('flayer edit would write it.')
  })

  it('says what validate would say of the text typed, word for word as flayer validate says it once written', async () => {
    const { page } = await render(EDITOR, commanding(fixture('workspace'), [], [...answers(), ...drafts()]).transport)

    await type(editor(page).view, validated.text)

    const verdict = page.get('[data-testid="verdict"]')
    expect(page.find('[data-testid="unsaved"]').exists()).toBe(true)
    expect(verdict.find('[data-testid="refused"]').exists()).toBe(false)
    const listed = verdict.findAll('[data-testid="issue"]').map(each => each.text().replace(/\s+/g, ' ').trim())
    const said = saidOfReview()
    expect(said).toHaveLength(2)
    expect(listed).toEqual(said.map(each => `${each.warning ? 'warning:' : 'invalid:'} ${each.message}`))
  })

  it('says why saving would be refused, by its code and in its words', async () => {
    const { page } = await render(EDITOR, commanding(fixture('workspace'), [], [...answers(), ...drafts()]).transport)
    const unfit = judged({ type: 'edit', text: review().text!.replace('description: Read a change before it lands', 'description: ""') })

    await type(editor(page).view, review().text!.replace('description: Read a change before it lands', 'description: ""'))

    const refused = page.get('[data-testid="refused"]')
    expect(refused.text()).toContain('flayer edit would refuse it')
    expect(refused.text()).toContain(unfit.refused!.code)
    expect(page.get('[data-testid="refused-message"]').text()).toBe(unfit.refused!.message)
    expect(page.get('[data-testid="verdict"]').findAll('[data-testid="issue"]').map(each => each.text())).toEqual(['invalid: `description` is empty'])
  })

  it('saves the text with the hash it was read with, as the line the server previews, from the button or the keyboard', async () => {
    const save = change('save')
    const { sent, transport } = commanding(fixture('workspace'), [save], [...answers(), ...drafts()])
    const { page } = await render(EDITOR, transport)
    const { view } = editor(page)

    await type(view, (save.task as { text: string }).text)
    await page.get('[data-testid="save"]').trigger('click')
    await flushPromises()

    expect(sent).toEqual([{ task: save.task, mode: 'preview' }])
    expect((save.task as { hash: string }).hash).toBe(review().hash)
    expect(shown()).toEqual(save.preview.lines.map(line => `$ ${line}`))
    document.body.querySelector<HTMLElement>('[data-testid="cancel"]')!.click()
    await flushPromises()

    // Ctrl-S, as the editor's keymap has it: the same task, shown first.
    view.contentDOM.dispatchEvent(new KeyboardEvent('keydown', { key: 's', code: 'KeyS', ctrlKey: true, bubbles: true, cancelable: true }))
    await flushPromises()
    expect(sent.at(-1)).toEqual({ task: save.task, mode: 'preview' })
    document.body.querySelector<HTMLElement>('[data-testid="run"]')!.click()
    await flushPromises()
    expect(sent.filter(each => each.mode === 'run')).toEqual([{ task: save.task, mode: 'run' }])
  })

  it('keeps what was typed and the hash it was read with when the file changes on disk, and the save is refused with core\'s message', async () => {
    const stale = change('save', 1)
    const answered: Answered[] = [...answers(), ...drafts()]
    const { sent, tell, transport } = commanding(fixture('workspace'), [stale], answered)
    const { page } = await render(EDITOR, transport)
    const typed = (stale.task as { text: string }).text
    await type(editor(page).view, typed)

    // Changed by somebody else: what the server answers now, and its word
    // that the library changed.
    const contents = answered.find(each => each.query.type === 'contents' && each.query.reference === 'skill:review' && each.query.file === 'SKILL.md')!
    const changed = { ...(contents.answer as Contents), text: `${review().text}\nChanged on disk since.\n`, hash: 'f'.repeat(64) }
    contents.answer = changed
    tell({ type: 'changed', scopes: [{ type: 'workspace' }] })
    await vi.waitFor(() => expect(page.find('[data-testid="changed"]').exists()).toBe(true))

    expect(editor(page).view.state.doc.toString()).toBe(typed)
    await page.get('[data-testid="save"]').trigger('click')
    await flushPromises()
    expect(sent[0]).toEqual({ task: stale.task, mode: 'preview' })
    document.body.querySelector<HTMLElement>('[data-testid="run"]')!.click()
    await flushPromises()

    const error = stale.ran.outcome.runs[0]!.error!
    expect(error.code).toBe('edit_stale')
    await vi.waitFor(() => expect(document.body.textContent).toContain(error.message))

    await page.get('[data-testid="read-again-changed"]').trigger('click')
    await flushPromises()
    expect(editor(page).view.state.doc.toString()).toBe(changed.text)
    expect(page.find('[data-testid="changed"]').exists()).toBe(false)
  })
})

describe('one of the library\'s own', () => {
  it('opens its editor from its page', async () => {
    const { page } = await render('/artifact?kind=skill&name=review', commanding(fixture('workspace'), [], [...answers(), ...drafts()]).transport)

    expect(page.get('[data-testid="edit"]').attributes('href')).toBe(EDITOR)
  })
})
