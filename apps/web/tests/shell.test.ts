import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createMemoryHistory } from 'vue-router'
import App from '@/App.vue'
import { createPanel } from '@/panel'

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
  it('draws the overview at the root', async () => {
    const page = await render('/')

    expect(page.get('h2').text()).toBe('Mindflayer')
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
