import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createMemoryHistory } from 'vue-router'
import App from '@/App.vue'
import { createPanel } from '@/panel'
import { useShell } from '@/stores/shell'
import type { Transport } from '@/api/transport'

/** A transport that writes down every call, and answers none of them. */
function recording() {
  const calls: string[] = []
  const transport: Transport = {
    query: () => {
      calls.push('query')
      return new Promise(() => {})
    },
    send: () => {
      calls.push('send')
      return new Promise(() => {})
    },
    subscribe: () => {
      calls.push('subscribe')
      return () => {}
    }
  } as Transport
  return { calls, transport }
}

let wrapper: VueWrapper | undefined
let styles: Element[]
let watching: MutationObserver

async function render(path: string, transport: Transport = recording().transport) {
  const panel = createPanel({ transport, history: createMemoryHistory() })
  await panel.router.push(path)
  wrapper = mount(App, { global: { plugins: [panel] }, attachTo: document.body })
  await flushPromises()
  return wrapper
}

beforeEach(() => {
  localStorage.clear()
  document.documentElement.className = 'dark'
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
    // `flayer list` leaves the shelf out: `gather list` is its listing.
    expect(page.findAll('[data-testid="commands"] code').map(code => code.text())).toEqual([
      '$ flayer list',
      '$ flayer gather list'
    ])
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

  it('keeps the sidebar under a name nothing rendered before it can move', async () => {
    const page = await render('/')

    await page.get('[data-testid="overview"] button[aria-label="Collapse sidebar"]').trigger('click')
    await flushPromises()

    expect(JSON.parse(localStorage.getItem('mindflayer-panel-sidebar-default') ?? '{}')).toMatchObject({ collapsed: true })
  })

  it('adds no style element, which the CSP would refuse', async () => {
    await render('/')

    // Not the palette, which Nuxt UI writes into one at run time, and not
    // the colour mode, which adds one for each switch.
    expect(styles).toEqual([])
  })

  it('asks the server nothing while it has no views', async () => {
    const { calls, transport } = recording()

    await render('/', transport)

    expect(calls).toEqual([])
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

describe('the colour mode', () => {
  it('starts dark when this browser has chosen nothing', async () => {
    await render('/')

    expect(document.documentElement.classList.contains('dark')).toBe(true)
    expect(localStorage.getItem('vueuse-color-scheme')).toBe('dark')
  })

  it('keeps the light mode this browser chose', async () => {
    localStorage.setItem('vueuse-color-scheme', 'light')

    await render('/')

    expect(document.documentElement.classList.contains('dark')).toBe(false)
    expect(document.documentElement.classList.contains('light')).toBe(true)
  })

  it('switches from the header without adding a style element', async () => {
    const page = await render('/')

    await page.get('[data-testid="overview"] button[aria-label="Switch to light mode"]').trigger('click')
    await flushPromises()

    expect(document.documentElement.classList.contains('light')).toBe(true)
    expect(document.documentElement.classList.contains('dark')).toBe(false)
    expect(localStorage.getItem('vueuse-color-scheme')).toBe('light')
    expect(styles).toEqual([])
  })
})
