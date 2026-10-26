import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createMemoryHistory } from 'vue-router'
import App from '@/App.vue'
import { createPanel } from '@/panel'
import { useShell } from '@/stores/shell'

let wrapper: VueWrapper | undefined
let styles: Element[]
let watching: MutationObserver

async function render(path: string) {
  const panel = createPanel({ history: createMemoryHistory() })
  await panel.router.push(path)
  wrapper = mount(App, { global: { plugins: [panel] }, attachTo: document.body })
  await flushPromises()
  return wrapper
}

beforeEach(() => {
  // The sidebar remembers whether it was collapsed.
  localStorage.clear()
  // Every <style> element anything adds, for as long as the test runs.
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
  wrapper?.unmount()
  wrapper = undefined
  document.body.innerHTML = ''
})

describe('the shell', () => {
  it('draws the sidebar and the header around an empty overview', async () => {
    const page = await render('/')

    const sidebar = page.get('[data-testid="sidebar"]')
    expect(sidebar.text()).toContain('Mindflayer')
    expect(sidebar.get('nav a[aria-current="page"]').text()).toBe('Overview')
    expect(sidebar.get('[data-testid="address"]').text()).toBe(window.location.host)

    expect(page.get('[data-testid="overview"] h1').text()).toBe('Overview')
    expect(page.text()).toContain('Nothing to show yet')
    expect(page.text()).toContain('$ flayer list')
  })

  it('answers a path it has no page for with one that says so', async () => {
    const page = await render('/projects/nowhere')

    expect(page.get('[data-testid="not-found"] h1').text()).toBe('Not found')
    expect(page.text()).toContain('The panel has nothing at /projects/nowhere.')
    expect(page.get('[data-testid="not-found"] a[href="/"]').text()).toContain('Back to the overview')
  })

  it('collapses the sidebar to its icons from the header', async () => {
    const page = await render('/')

    await page.get('[data-testid="overview"] button[aria-label="Collapse sidebar"]').trigger('click')
    await flushPromises()

    expect(useShell().sidebarCollapsed).toBe(true)
    expect(page.get('[data-testid="sidebar"]').text()).not.toContain('Mindflayer')
    expect(page.get('[data-testid="address"]').text()).toBe('')
  })

  it('writes its palette into no style element, which the CSP would refuse', async () => {
    await render('/')

    expect(styles.filter(style => style.textContent?.includes('--ui-color-'))).toEqual([])
  })

  it('draws every icon it names from the bundle', async () => {
    const page = await render('/')

    // Iconify draws an icon it holds as an <svg>, and nothing at all for one
    // it would have had to fetch.
    const icons = page.findAllComponents({ name: 'Icon' })
    expect(icons.length).toBeGreaterThan(0)
    for (const icon of icons) {
      expect(icon.element).toBeInstanceOf(SVGElement)
      expect((icon.element as SVGElement).innerHTML).not.toBe('')
    }
  })
})
