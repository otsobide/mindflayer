import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { flushPromises, type VueWrapper } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import { useRunner, type Runner } from '@/api/commands'
import WorkspaceSwitcher from '@/components/WorkspaceSwitcher.vue'
import { useMarks } from '@/stores/marks'
import { commanded, commanding, fixture, render, switches, switching, unmount } from './panel'

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
  expect(styles, 'a <style> the CSP would refuse').toEqual([])
})

let runner: Runner

/**
 * A page of the test's own: the switcher, and the panel's runner handed to
 * the test, to show a task with.
 */
const Harness = defineComponent({
  setup() {
    runner = useRunner()
    return () => h('main', { 'data-testid': 'harness' }, [h(WorkspaceSwitcher)])
  }
})

const HARNESS = [{ path: '/harness', component: Harness }]

/*
 * The header's switcher, drawn from what the CLI's suite wrote (tests/
 * fixtures/switches.json): the workspace the matrix draws, `Assign`, found
 * above where the panel was launched, in a home whose default workspace
 * knows it, `team`, `kit` and `old`, gone since; switched to `kit`, and
 * refused `old`.
 */

const FIXTURES = switches()

/** Open the switcher, as a key on it opens it, and wait for what it lists. */
async function opened(page: VueWrapper) {
  await page.get('[data-testid="workspace-switcher"]').trigger('keydown', { key: 'Enter' })
  await flushPromises()
}

/** Each item of the open switcher. */
function items(): HTMLElement[] {
  return [...document.body.querySelectorAll<HTMLElement>('[role="menuitemcheckbox"]')]
}

/** Each workspace the open switcher lists: its name, its words, whether it is checked, and whether it can be chosen. */
function listed() {
  return items().map(item => ({
    name: item.querySelector('[data-slot="itemLabel"]')?.textContent,
    says: item.querySelector('[data-slot="itemDescription"]')?.textContent,
    checked: item.getAttribute('aria-checked'),
    chosen: !item.hasAttribute('data-disabled')
  }))
}

/** The item of the open switcher naming `name`. */
function item(name: string): HTMLElement {
  const found = items().find(each => each.querySelector('[data-slot="itemLabel"]')?.textContent === name)
  if (!found) {
    throw new Error(`the switcher lists no ${name}`)
  }
  return found
}

/** Every toast's title and description, as text. */
function toasted(): string {
  return [...document.body.querySelectorAll('[data-slot="title"], [data-slot="description"]')].map(each => each.textContent).join('\n')
}

/** How many times the snapshot was asked for. */
function snapshots(queries: { type: string }[]): number {
  return queries.filter(query => query.type === 'snapshot').length
}

describe('the workspace switcher', () => {
  it('names the workspace in use and the rule that chose it, in words, on every page', async () => {
    for (const path of ['/', '/projects', '/library', '/loads']) {
      const { page } = await render(path, switching().transport)

      const switcher = page.get('[data-testid="workspace-switcher"]')
      expect(switcher.get('[data-testid="workspace-name"]').text()).toBe('Assign')
      expect(switcher.get('[data-testid="workspace-reason"]').text()).toBe('found above')
      expect(switcher.text().replace(/\s+/g, ' ')).toBe('Switch workspace; in use: Assign, found above')
      unmount()
    }
  })

  it('asks what it may switch to only once opened, and lists each as core does: the one in use checked, the one gone offered nothing', async () => {
    const { transport, queries } = switching()
    const { page } = await render('/', transport)
    expect(queries.map(query => query.type)).not.toContain('workspaces')

    await opened(page)

    expect(queries.map(query => query.type)).toContain('workspaces')
    const old = (FIXTURES.workspaces.answer as { stale: string | null }[])[4]!.stale
    expect(listed()).toEqual([
      { name: 'you', says: '/home/you\nthe default · 0 projects', checked: 'false', chosen: true },
      { name: 'Assign', says: '/home/you/Assign\nin use · launched here, found above · 5 projects, 1 stale', checked: 'true', chosen: false },
      { name: 'team', says: '/home/you/team\n0 projects', checked: 'false', chosen: true },
      { name: 'kit', says: '/home/you/kit\n0 projects', checked: 'false', chosen: true },
      { name: '/home/you/old', says: `/home/you/old\ndoes not open: ${old}`, checked: 'false', chosen: false }
    ])
  })

  it('switches to the one chosen by its path as listed, then draws the overview of it, named as a switch names it', async () => {
    const { transport, sent, queries } = switching()
    const { page, router } = await render('/projects', transport)
    await opened(page)
    const before = snapshots(queries)

    item('kit').click()
    await flushPromises()

    expect(sent).toEqual([FIXTURES.switched.to])
    expect(snapshots(queries)).toBeGreaterThan(before)
    expect(router.currentRoute.value.path).toBe('/')
    const switcher = page.get('[data-testid="workspace-switcher"]')
    expect(switcher.get('[data-testid="workspace-name"]').text()).toBe('kit')
    expect(switcher.get('[data-testid="workspace-reason"]').text()).toBe('named with -W')
    expect(page.get('[data-testid="workspace-path"]').text()).toBe('/home/you/kit')
    expect(toasted()).toContain('Switched to kit')
    expect(toasted()).toContain('-W /home/you/kit')

    await opened(page)
    expect(listed().map(each => [each.name, each.checked])).toEqual([
      ['you', 'false'],
      ['Assign', 'false'],
      ['team', 'false'],
      ['kit', 'true'],
      ['/home/you/old', 'false']
    ])
    expect(item('Assign').querySelector('[data-slot="itemDescription"]')?.textContent).toContain('launched here, found above')
  })

  it('says why a switch was refused, by its code, and leaves the panel as it was', async () => {
    const { transport, sent } = switching()
    // The list as a page read it before `kit` went: offered, then refused
    // as the server refuses one gone since.
    const refusing = { ...transport, switch: () => transport.switch(FIXTURES.refused.to) }
    const { page, router } = await render('/projects', refusing)
    await opened(page)

    item('kit').click()
    await flushPromises()

    expect(sent).toEqual([FIXTURES.refused.to])
    expect(router.currentRoute.value.path).toBe('/projects')
    expect(page.get('[data-testid="workspace-name"]').text()).toBe('Assign')
    expect(toasted()).toContain('Not switched')
    expect(toasted()).toContain(`${FIXTURES.refused.error} (${FIXTURES.refused.code})`)
  })

  it('cannot be opened while a task is shown, whose lines name the workspace in use', async () => {
    const [link] = commanded()
    const { transport } = commanding(fixture('workspace'), [link!])
    const { page } = await render('/harness', transport, HARNESS)
    const switcher = () => page.get('[data-testid="harness"] [data-testid="workspace-switcher"]')
    expect(switcher().attributes('disabled')).toBeUndefined()

    void runner.ask(link!.task)
    await flushPromises()

    expect(runner.asking.value?.task).toEqual(link!.task)
    expect(switcher().attributes('disabled')).toBeDefined()
    runner.cancel()
    await flushPromises()
    expect(switcher().attributes('disabled')).toBeUndefined()
  })
})

describe('a switch told on the stream', () => {
  it('reads everything again, lets a task shown go and forgets the boxes marked, whichever tab switched', async () => {
    const [link] = commanded()
    const { transport, queries, tell } = commanding(fixture('workspace'), [link!])
    const { page } = await render('/harness', transport, HARNESS)
    const marks = page.vm.$.appContext.app.runWithContext(() => useMarks())
    marks.mark({ project: 'api', reference: 'skill:review', origin: 'workspace', wanted: false }, true)
    void runner.ask(link!.task)
    await flushPromises()
    expect(marks.marks).toHaveLength(1)
    expect(runner.asking.value).toBeDefined()
    const before = snapshots(queries)

    tell({ type: 'switched' })
    await flushPromises()

    expect(runner.asking.value).toBeUndefined()
    expect(marks.marks).toEqual([])
    expect(snapshots(queries)).toBeGreaterThan(before)
  })
})
