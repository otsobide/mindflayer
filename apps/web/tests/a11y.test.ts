import { afterEach, describe, expect, it } from 'vitest'
import axe from 'axe-core'
import { fixture, render, unmount, type Fixture } from './panel'

afterEach(unmount)

/**
 * What axe finds wrong with the page as it is drawn now, every WCAG A and AA
 * rule and its best practices, as `id: what is wrong, where`.
 *
 * Contrast is left out: happy-dom lays nothing out and computes no colour,
 * so axe cannot measure it here, and says so rather than passing it. The
 * palette's contrast is Nuxt UI's, which tests its own.
 */
async function violations(): Promise<string[]> {
  const results = await axe.run(document.body, {
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
