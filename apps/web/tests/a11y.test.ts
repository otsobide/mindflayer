import { afterEach, describe, expect, it } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import axe from 'axe-core'
import type { LiveEvent, Transport } from '@/api/transport'
import { useShell } from '@/stores/shell'
import { answering, commanded, commanding, fixture, render, unmount, type Fixture } from './panel'

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
  ['/nowhere', 'workspace']
]

describe('every page', () => {
  it.each(PAGES)('%s, of the %s fixture, passes axe', async (path, name) => {
    await render(path, fixture(name))

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
    const { ran, transport } = commanding()
    const tasks = commanded()
    ran.push(...tasks.map(each => each.ran))
    await render('/projects', transport)

    document.querySelector<HTMLElement>('[data-testid="open-transcript"]')!.click()
    await flushPromises()
    expect(document.querySelector('[data-testid="transcript"]')).not.toBeNull()
    expect(await violations(OVER)).toEqual([])

    useShell().transcriptOpen = false
    useShell().warningsOpen = true
    await flushPromises()
    expect(document.querySelector('[data-testid="nothing-ran"]')).not.toBeNull()
    expect(await violations(OVER)).toEqual([])
  })
})
