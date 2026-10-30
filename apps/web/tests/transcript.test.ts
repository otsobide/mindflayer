import { afterEach, describe, expect, it } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import { useRunner, type Runner } from '@/api/commands'
import { useSnapshot } from '@/api/snapshot'
import { useShell } from '@/stores/shell'
import { commanded, commanding, render, unmount } from './panel'

/*
 * The transcript drawer, opened from any page's header, over a transport
 * answering each task as the backend answered it
 * (tests/fixtures/commands.json): what it lists of each run, and the
 * script it copies, which is the lines the server previewed for each task,
 * and nothing else.
 */

afterEach(() => {
  unmount()
})

function find(selector: string): HTMLElement | null {
  return document.body.querySelector<HTMLElement>(selector)
}

function findAll(selector: string): HTMLElement[] {
  return [...document.body.querySelectorAll<HTMLElement>(selector)]
}

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

/** How many times the transcript was asked for. */
function counting(transport: ReturnType<typeof commanding>['transport']) {
  const asked = { times: 0 }
  const transcript = transport.transcript.bind(transport)
  transport.transcript = () => {
    asked.times += 1
    return transcript()
  }
  return asked
}

describe('the transcript', () => {
  it('is asked for only once its drawer is opened from a page\'s header', async () => {
    const { transport } = commanding()
    const asked = counting(transport)
    await render('/projects', transport)
    expect(asked.times).toBe(0)

    await click('[data-testid="projects"] [data-testid="open-transcript"]')

    expect(asked.times).toBe(1)
    expect(text(find('[data-testid="transcript-empty"]'))).toBe('Nothing has run yet this session.')
  })

  it('lists every task the session ran, each line with what it said', async () => {
    const { ran, transport } = commanding()
    const tasks = commanded()
    ran.push(...tasks.map(each => each.ran))
    await render('/', transport)

    await click('[data-testid="open-transcript"]')

    const entries = findAll('[data-testid="entry"]')
    expect(entries).toHaveLength(tasks.length)
    entries.forEach((entry, index) => {
      const { ran } = tasks[index]!
      const steps = [...entry.querySelectorAll('[data-testid="step"]')]
      expect(steps.map(step => text(step.querySelector('[data-testid="line"]')))).toEqual(ran.lines.map(line => `$ ${line}`))
      expect(steps.map(step => text(step.querySelector('[data-testid="ok"]')))).toEqual(ran.outcome.runs.map(run => (run.ok ? 'ok' : 'failed')))
    })
    const validate = entries[2]!.querySelector('[data-testid="said"]')!
    const [run] = tasks[2]!.ran.outcome.runs
    expect(validate.textContent).toBe(`${run!.stdout}${run!.stderr.map(line => `${line.text}\n`).join('')}`)
  })

  it('copies as a script exactly the lines the server previewed for each task, one to a line', async () => {
    const { ran, transport } = commanding()
    const tasks = commanded()
    ran.push(...tasks.map(each => each.ran))
    await render('/', transport)
    await click('[data-testid="open-transcript"]')

    await click('[data-testid="copy"]')

    const copied = await navigator.clipboard.readText()
    expect(copied.split('\n')).toEqual([...tasks.flatMap(each => each.preview.lines), ''])
    expect(copied.split('\n').slice(0, -1)).toEqual(tasks.flatMap(each => each.ran.lines))
    expect(text(document.body)).toContain(`Copied ${tasks.flatMap(each => each.ran.lines).length} lines`)
  })

  it('has nothing to copy before anything ran', async () => {
    const { transport } = commanding()
    await render('/', transport)

    await click('[data-testid="open-transcript"]')

    expect(find('[data-testid="copy"]')?.hasAttribute('disabled')).toBe(true)
  })

  it('shows a run the panel sends while it is open, once the run finished', async () => {
    let runner: Runner | undefined
    const Harness = defineComponent({
      setup() {
        runner = useRunner()
        useSnapshot()
        return () => h('main', 'harness')
      }
    })
    const { transport } = commanding()
    const asked = counting(transport)
    await render('/harness', transport, [{ path: '/harness', component: Harness }])
    useShell().transcriptOpen = true
    await flushPromises()
    const [link] = commanded()

    const answer = runner!.ask(link!.task)
    await flushPromises()
    await click('[data-testid="run"]')
    await answer
    await flushPromises()

    expect(asked.times).toBe(2)
    expect(findAll('[data-testid="entry"] [data-testid="line"]').map(text)).toEqual(link!.ran.lines.map(line => `$ ${line}`))
  })
})
