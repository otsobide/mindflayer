import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import { useRunner, type Runner } from '@/api/commands'
import { useSnapshot } from '@/api/snapshot'
import { TransportError, type Ran, type Task, type Transport } from '@/api/transport'
import { useShell } from '@/stores/shell'
import { commanded, commanding, render, unmount } from './panel'

/*
 * The runner every page changes things through, driven from a page of the
 * test's own in the panel's frame, over a transport answering each task as
 * the backend answered it (tests/fixtures/commands.json): what it shows
 * before a run, the question a task that takes something away is, the
 * toast and the drawer a run ends with, and what it waits for before it
 * says a run is over.
 */

let runner: Runner

/**
 * A page of the test's own, handing the test the panel's runner, and
 * reading the workspace as every page does.
 */
const Harness = defineComponent({
  setup() {
    runner = useRunner()
    const { data } = useSnapshot()
    return () => h('main', { 'data-testid': 'harness' }, data.value?.workspace.name ?? 'reading')
  }
})

const HARNESS = [{ path: '/harness', component: Harness }]

const [LINK, UNLINK, VALIDATE, REMOVE] = commanded() as [ReturnType<typeof commanded>[number], ReturnType<typeof commanded>[number], ReturnType<typeof commanded>[number], ReturnType<typeof commanded>[number]]

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

/** The element `selector` finds anywhere on the page, the dialogs and toasts teleported to its body among it. */
function find(selector: string): HTMLElement | null {
  return document.body.querySelector<HTMLElement>(selector)
}

function findAll(selector: string): HTMLElement[] {
  return [...document.body.querySelectorAll<HTMLElement>(selector)]
}

/** The text of `element`, its spaces run together. */
function text(element: Element | null): string {
  return (element?.textContent ?? '').replace(/\s+/g, ' ').trim()
}

async function click(selector: string) {
  const element = find(selector)
  if (!element) {
    throw new Error(`nothing on the page is ${selector}`)
  }
  element.click()
  await flushPromises()
}

/**
 * Ask the runner for `task`, and wait until what it shows is drawn: how it
 * ends, which only an answer to what is shown brings, comes in `answer`.
 */
async function ask(task: Task): Promise<{ answer: Promise<Ran | undefined> }> {
  const answer = runner.ask(task)
  await flushPromises()
  return { answer }
}

describe('a task the panel runs', () => {
  it('is shown first as the lines the server previewed, and runs nothing until asked', async () => {
    const { sent, transport } = commanding()
    await render('/harness', transport, HARNESS)

    const { answer: asked } = await ask(LINK.task)

    expect(findAll('[data-testid="command"]').map(text)).toEqual(LINK.preview.lines.map(line => `$ ${line}`))
    expect(findAll('[data-testid="caution"]')).toEqual([])
    expect(text(find('[data-testid="run"]'))).toBe('Run')
    expect(sent).toEqual([{ task: LINK.task, mode: 'preview' }])

    await click('[data-testid="run"]')

    expect(await asked).toEqual(LINK.ran)
    expect(sent).toEqual([{ task: LINK.task, mode: 'preview' }, { task: LINK.task, mode: 'run' }])
    expect(find('[data-testid="preview"]')).toBeNull()
    expect(styles).toEqual([])
  })

  it('says what its run produced in a toast', async () => {
    const { transport } = commanding()
    await render('/harness', transport, HARNESS)

    const { answer: asked } = await ask(LINK.task)
    await click('[data-testid="run"]')
    await asked

    const toast = text(document.body.querySelector('[data-slot="root"][data-state="open"]') ?? document.body)
    expect(toast).toContain(`Ran ${LINK.ran.lines[0]}`)
    expect(toast).toContain(LINK.ran.outcome.runs[0]!.stdout.trim())
  })

  it('is let go with nothing run when somebody says no', async () => {
    const { sent, transport } = commanding()
    await render('/harness', transport, HARNESS)

    const { answer: asked } = await ask(LINK.task)
    await click('[data-testid="cancel"]')

    expect(await asked).toBeUndefined()
    expect(sent.map(each => each.mode)).toEqual(['preview'])
    expect(find('[data-testid="preview"]')).toBeNull()
  })

  it('refused by the backend runs nothing, and says why with its code', async () => {
    const { sent, transport } = commanding()
    await render('/harness', transport, HARNESS)
    const task: Task = { type: 'link', path: 'relative/blog' }

    expect(await runner.ask(task)).toBeUndefined()
    await flushPromises()

    expect(sent).toEqual([{ task, mode: 'preview' }])
    expect(find('[data-testid="preview"]')).toBeNull()
    expect(text(document.body)).toContain('Nothing ran')
    expect(text(document.body)).toContain('(not_found)')
  })

  it('is over once the stream says it finished and the views have read the workspace again', async () => {
    const { control, queries, transport } = commanding()
    await render('/harness', transport, HARNESS)
    let release: () => void = () => {}
    control.finishing = new Promise((resolve) => {
      release = resolve
    })
    let over = false

    const { answer: asked } = await ask(LINK.task)
    void asked.then(() => (over = true))
    const snapshots = queries.filter(query => query.type === 'snapshot').length
    await click('[data-testid="run"]')

    expect(runner.running.value).toBe(true)
    expect(over).toBe(false)
    expect(text(document.body)).not.toContain(`Ran ${LINK.ran.lines[0]}`)
    release()
    await asked
    await flushPromises()

    expect(over).toBe(true)
    expect(queries.filter(query => query.type === 'snapshot').length).toBeGreaterThan(snapshots)
    expect(text(document.body)).toContain(`Ran ${LINK.ran.lines[0]}`)
  })

  it('shows nothing else while one is shown or running', async () => {
    const { sent, transport } = commanding()
    await render('/harness', transport, HARNESS)

    await ask(LINK.task)

    expect(await runner.ask(UNLINK.task)).toBeUndefined()
    expect(sent.map(each => each.task.type)).toEqual(['link'])
  })
})

describe('a task that takes something away', () => {
  it('cannot run without somebody confirming what it takes, in the backend\'s words', async () => {
    const { sent, transport } = commanding()
    await render('/harness', transport, HARNESS)

    const { answer: asked } = await ask(UNLINK.task)

    expect(runner.confirming.value).toBe(true)
    expect(text(find('[role="dialog"] h2'))).toBe('Confirm what this takes away')
    const cautions = findAll('[data-testid="caution"]')
    expect(cautions.map(text)).toEqual(UNLINK.preview.confirm.map(each => `${each.caution}: ${each.message}`))
    expect(cautions[0]!.querySelector('[data-icon]')?.getAttribute('data-icon')).toBe('i-lucide-unlink')
    expect(text(find('[data-testid="run"]'))).toBe('Confirm and run')
    expect(document.activeElement).toBe(find('[data-testid="cancel"]'))

    // Escape, as a slip of the hand would, is a no.
    document.activeElement?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await flushPromises()
    expect(await asked).toBeUndefined()
    expect(sent.filter(each => each.mode === 'run')).toEqual([])

    const { answer: again } = await ask(UNLINK.task)
    await click('[data-testid="run"]')
    expect(await again).toEqual(UNLINK.ran)
    expect(sent.filter(each => each.mode === 'run')).toEqual([{ task: UNLINK.task, mode: 'run' }])
  })

  it('names each thing a line takes away beside that line', async () => {
    const { transport } = commanding()
    await render('/harness', transport, HARNESS)

    await ask(REMOVE.task)

    const lines = findAll('[data-testid="line"]')
    expect(lines).toHaveLength(REMOVE.preview.lines.length)
    expect([...lines[0]!.querySelectorAll('[data-testid="caution"]')].map(text)).toEqual([
      `removes: ${REMOVE.preview.confirm[0]!.message}`,
      `uninstalls: ${REMOVE.preview.confirm[1]!.message}`
    ])
  })
})

describe('what a run said', () => {
  it('opens the warnings drawer when it failed, each line with an icon chosen by its code', async () => {
    const { transport } = commanding()
    await render('/harness', transport, HARNESS)

    const { answer: asked } = await ask(VALIDATE.task)
    await click('[data-testid="run"]')
    await asked

    expect(useShell().warningsOpen).toBe(true)
    const runs = findAll('[data-testid="said"] [data-testid="run"]')
    expect(runs).toHaveLength(1)
    expect(text(runs[0]!.querySelector('[data-testid="ok"]'))).toBe('failed')
    const [line] = VALIDATE.ran.outcome.runs[0]!.stderr
    const said = runs[0]!.querySelector('[data-testid="line"]')!
    expect(text(said)).toBe(`${line!.code}: ${line!.text}`)
    expect(said.querySelector('[data-icon]')?.getAttribute('data-icon')).toBe('i-lucide-file-warning')
  })

  it('draws a code it knows no icon for as a warning, and a line with none as a note', async () => {
    const validate = commanded()[2]!
    validate.ran.outcome.runs[0]!.stderr = [
      { code: 'a_code_from_later', text: 'warning: something new' },
      { code: null, text: 'note: nothing keeps its copies out of git' }
    ]
    const { transport } = commanding(undefined, [validate])
    await render('/harness', transport, HARNESS)

    const { answer: asked } = await ask(validate.task)
    await click('[data-testid="run"]')
    await asked

    const icons = findAll('[data-testid="said"] [data-testid="line"] [data-icon]').map(icon => icon.getAttribute('data-icon'))
    expect(icons).toEqual(['i-lucide-triangle-alert', 'i-lucide-info'])
  })

  it('is still the last run that answered after a later run is refused, or never answers', async () => {
    const { transport } = commanding()
    let failing = new TransportError(422, 'another writer holds the workspace', 'workspace_busy')
    const refusing = {
      ...transport,
      send(task: Task, mode: 'preview' | 'run') {
        if (mode === 'preview') {
          return transport.send(task, mode)
        }
        return task.type === 'link' ? Promise.reject(failing) : transport.send(task, mode)
      }
    } as Transport
    await render('/harness', refusing, HARNESS)

    const { answer: validated } = await ask(VALIDATE.task)
    await click('[data-testid="run"]')
    await validated
    useShell().warningsOpen = false

    for (const failure of [failing, new TransportError(500, 'the server failed'), new TransportError(0, 'the server stopped')]) {
      failing = failure
      const { answer: linked } = await ask(LINK.task)
      await click('[data-testid="run"]')
      expect(await linked).toBeUndefined()

      useShell().warningsOpen = true
      await flushPromises()
      expect(find('[data-testid="nothing-ran"]')).toBeNull()
      expect(findAll('[data-testid="said"] [data-testid="run"] code').map(text)).toEqual(VALIDATE.ran.outcome.runs.map(run => `$ ${run.command}`))
      useShell().warningsOpen = false
      await flushPromises()
    }
  })

  it('leaves the drawer closed after a run that went well and said nothing', async () => {
    const { transport } = commanding()
    await render('/harness', transport, HARNESS)

    const { answer: asked } = await ask(LINK.task)
    await click('[data-testid="run"]')
    await asked

    expect(useShell().warningsOpen).toBe(false)
  })
})
