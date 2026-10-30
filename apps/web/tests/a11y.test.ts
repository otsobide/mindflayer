import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import axe from 'axe-core'
import { defineComponent, h } from 'vue'
import { useRunner, type Runner } from '@/api/commands'
import type { LiveEvent, Transport } from '@/api/transport'
import Projects from '@/pages/projects.vue'
import { useShell } from '@/stores/shell'
import { answering, answers, commanded, commanding, fixture, plans, render, unmount, updates, type Fixture } from './panel'

afterEach(unmount)

/**
 * What axe finds wrong with the page as it is drawn now, every WCAG A and AA
 * rule and its best practices, as `id: what is wrong, where`.
 *
 * Contrast is left out: happy-dom lays nothing out and computes no colour,
 * so axe cannot measure it here, and says so rather than passing it. The
 * palette's contrast is Nuxt UI's, which tests its own.
 */
async function violations(context: axe.ElementContext = document.body): Promise<string[]> {
  const results = await axe.run(context, {
    runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa', 'best-practice'] },
    rules: { 'color-contrast': { enabled: false } }
  })
  return results.violations.flatMap(violation =>
    violation.nodes.map(node => `${violation.id}: ${violation.help}, at ${node.html.slice(0, 160)}`)
  )
}

const PAGES: [string, Fixture][] = [
  ['/', 'workspace'],
  ['/', 'unmade'],
  ['/projects', 'workspace'],
  ['/projects?q=nowhere', 'workspace'],
  ['/projects', 'unmade'],
  ['/project?entry=api', 'workspace'],
  ['/project?entry=web', 'workspace'],
  ['/project?entry=docs', 'workspace'],
  ['/project?entry=gone', 'workspace'],
  ['/project?entry=nowhere', 'workspace'],
  ['/project?entry=legacy', 'candidates'],
  ['/library', 'workspace'],
  ['/library?kind=rule', 'workspace'],
  ['/library?kind=rule&q=nowhere', 'workspace'],
  ['/library?kind=agent', 'workspace'],
  ['/library', 'unmade'],
  ['/artifact?kind=skill&name=review', 'workspace'],
  ['/artifact?kind=skill&name=review&file=references/checklist.md', 'workspace'],
  ['/artifact?kind=rule&name=git/commits', 'workspace'],
  ['/artifact?kind=rule&name=style/comments', 'workspace'],
  ['/artifact?kind=skill&name=nowhere', 'workspace'],
  ['/shelf', 'workspace'],
  ['/shelf?kind=rule', 'workspace'],
  ['/shelf', 'unmade'],
  ['/sources', 'workspace'],
  ['/sources', 'unmade'],
  ['/discover', 'workspace'],
  ['/discover', 'unmade'],
  ['/discover?root=%2Fhome%2Fyou%2FProjects', 'candidates'],
  ['/discover?root=Projects', 'workspace'],
  ['/assign', 'assigning'],
  ['/assign?by=project', 'assigning'],
  ['/assign?q=nowhere', 'assigning'],
  ['/assign', 'workspace'],
  ['/assign', 'unmade'],
  ['/projects', 'updating'],
  ['/nowhere', 'workspace']
]

describe('every page', () => {
  it.each(PAGES)('%s, of the %s fixture, passes axe', async (path, name) => {
    await render(path, fixture(name))

    expect(await violations()).toEqual([])
  })
})

describe('a project\'s toggles', () => {
  it('pass axe turned over, ready to apply', async () => {
    await render('/project?entry=web', fixture('workspace'))

    document.querySelector<HTMLElement>('[data-testid="agent-codex"] [role="checkbox"], [role="checkbox"][data-testid="agent-codex"]')!.click()
    await flushPromises()

    expect(document.querySelector('[data-testid="apply-settings"]')?.hasAttribute('disabled')).toBe(false)
    expect(await violations()).toEqual([])
  })
})

describe('a project an older marker marks', () => {
  it('passes axe with what migrating it would do shown', async () => {
    const { transport } = commanding(fixture('candidates'))
    await render('/project?entry=legacy', transport)

    document.querySelector<HTMLElement>('[data-testid="migrate-dry"]')!.click()
    await flushPromises()
    document.querySelector<HTMLElement>('[data-testid="run"]')!.click()
    await vi.waitFor(() => expect(document.querySelector('[data-testid="migrate-said"]')).not.toBeNull())

    expect(await violations()).toEqual([])
  })
})

describe('the page of what could be linked', () => {
  it('passes axe with folders marked, one a repository mind init writes into', async () => {
    await render('/discover?root=%2Fhome%2Fyou%2FProjects', fixture('candidates'))

    for (const box of document.querySelectorAll<HTMLElement>('[data-testid="suggestion"] [role="checkbox"]')) {
      box.click()
    }
    await flushPromises()

    expect(document.querySelector('[data-testid="writes-into"]')).not.toBeNull()
    expect(await violations()).toEqual([])
  })
})

describe('the matrix', () => {
  it('passes axe with boxes marked, and the plan of them drawn, held back and clashing', async () => {
    const { transport } = answering(fixture('assigning'), plans())
    await render('/assign', transport)

    const [, , heldBack] = plans()
    for (const cell of (heldBack!.query as { selection: { project: string, reference: string, origin: string }[] }).selection) {
      const label = `${cell.reference.replace(':', ' ')} from ${cell.origin} in ${cell.project}`
      const box = [...document.querySelectorAll<HTMLElement>('[data-testid="box"]')]
        .find(each => each.getAttribute('aria-label')!.split('; ')[0] === label)!
      box.click()
      await flushPromises()
    }

    expect(document.querySelector('[data-testid="lock"]')).not.toBeNull()
    expect(document.querySelector('[data-testid="conflict"]')).not.toBeNull()
    expect(await violations()).toEqual([])
  })
})

describe('the page of updates', () => {
  it('passes axe with every state listed, and with nothing to update', async () => {
    const { transport } = answering(fixture('updating'), updates())
    await render('/updates', transport)

    expect(document.querySelector('[data-testid="state-edited"] [data-testid="row"]')).not.toBeNull()
    expect(await violations()).toEqual([])
    unmount()

    const nothing = updates()
    Object.assign(nothing[0]!.answer as object, { projects: [], unread: [], writes: false })
    await render('/updates', answering(fixture('workspace'), nothing).transport)

    expect(document.querySelector('[data-testid="nothing"]')).not.toBeNull()
    expect(await violations()).toEqual([])
  })

  it('passes axe once what an update would do could not be read', async () => {
    await render('/updates', answering(fixture('workspace'), []).transport)

    expect(document.querySelector('[data-testid="updates-unread"]')).not.toBeNull()
    expect(await violations()).toEqual([])
  })
})

describe('one of the library\'s own, about to be removed', () => {
  it('passes axe with what removing it everywhere would do shown', async () => {
    const { transport } = commanding(fixture('workspace'), commanded(), answers())
    await render('/artifact?kind=skill&name=review', transport)

    document.querySelector<HTMLElement>('[data-testid="remove-dry"]')!.click()
    await flushPromises()
    document.querySelector<HTMLElement>('[data-testid="run"]')!.click()
    await vi.waitFor(() => expect(document.querySelector('[data-testid="remove-said"]')).not.toBeNull())

    expect(await violations()).toEqual([])
  })
})

describe('a page over a workspace the server cannot watch all of', () => {
  it('passes axe with the banner that says so', async () => {
    const { transport } = answering(fixture('workspace'))
    const unwatched: LiveEvent = { type: 'unwatched', reason: 'the system\'s limit on watched folders is reached' }
    await render('/', {
      ...transport,
      subscribe(listener: (event: LiveEvent) => void) {
        listener(unwatched)
        return () => {}
      }
    } as Transport)

    expect(document.querySelector('[data-testid="unwatched"]')).not.toBeNull()
    expect(await violations()).toEqual([])
  })
})

/**
 * The page with a modal open over it: everything but what Reka hides from
 * assistive technology while the modal holds the focus, which it marks
 * `aria-hidden` and leaves focusable underneath, the focus kept in the
 * modal meanwhile. axe would call that a hidden element holding focusable
 * ones, a pattern of the library's own, not of a page.
 */
const OVER: axe.ElementContext = { include: [document.body], exclude: [['[data-aria-hidden="true"]']] }

let runner: Runner

/**
 * The projects page, drawn by a page of the test's own that hands the test
 * the panel's runner, to run a task no page offers: the failed validate
 * whose drawer is checked.
 */
const Harness = defineComponent({
  setup() {
    runner = useRunner()
    return () => h(Projects)
  }
})

const HARNESS = [{ path: '/harness', component: Harness }]

describe('what the runner draws over a page', () => {
  it('passes axe with a confirmation open', async () => {
    const { transport } = commanding()
    await render('/project?entry=gone', transport)

    document.querySelector<HTMLElement>('[data-testid="unlink"]')!.click()
    await flushPromises()

    expect(document.querySelector('[data-testid="caution"]')).not.toBeNull()
    expect(await violations(OVER)).toEqual([])
  })

  it('passes axe with the transcript open, and with what a failed run said', async () => {
    const tasks = commanded()
    const validate = tasks[2]!
    // Every way the drawer draws a run beside the fixture's failed one, a
    // line said with a code: a note and why a line could not run, and a
    // line that said nothing beside its output.
    const more = [
      {
        command: 'mind -C /home/you/Projects/docs list',
        stdout: '',
        stderr: [{ code: null, text: 'note: nothing keeps its copies out of git' }],
        error: { code: 'workspace_busy', message: 'another writer holds the workspace' },
        ok: false
      },
      { command: 'mind -C /home/you/Projects/docs status', stdout: 'docs: nothing installed\n', stderr: [], error: null, ok: true }
    ]
    validate.ran.outcome.runs.push(...more)
    validate.ran.lines.push(...more.map(run => run.command))
    const { ran, transport } = commanding(undefined, tasks)
    ran.push(...tasks.map(each => each.ran))
    await render('/harness', transport, HARNESS)

    document.querySelector<HTMLElement>('[data-testid="open-transcript"]')!.click()
    await flushPromises()
    expect(document.querySelector('[data-testid="transcript"]')).not.toBeNull()
    expect(await violations(OVER)).toEqual([])

    useShell().transcriptOpen = false
    useShell().warningsOpen = true
    await flushPromises()
    expect(document.querySelector('[data-testid="nothing-ran"]')).not.toBeNull()
    expect(await violations(OVER)).toEqual([])

    useShell().warningsOpen = false
    await flushPromises()
    const asked = runner.ask(validate.task)
    await flushPromises()
    document.querySelector<HTMLElement>('[data-testid="run"]')!.click()
    await asked
    await flushPromises()
    expect(useShell().warningsOpen).toBe(true)
    expect(document.querySelectorAll('[data-testid="said"] [data-testid="run"]')).toHaveLength(validate.ran.outcome.runs.length)
    expect(await violations(OVER)).toEqual([])
  })
})
