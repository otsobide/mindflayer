import { afterEach, describe, expect, it } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createMemoryHistory } from 'vue-router'
import App from '@/App.vue'
import { createPanel } from '@/panel'

let wrapper: VueWrapper | undefined

async function render(path: string) {
  const panel = createPanel({ history: createMemoryHistory() })
  await panel.router.push(path)
  wrapper = mount(App, { global: { plugins: [panel] }, attachTo: document.body })
  await flushPromises()
  return wrapper
}

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
  document.body.innerHTML = ''
})

describe('the shell', () => {
  it('draws the overview at the root', async () => {
    const page = await render('/')

    expect(page.get('h2').text()).toBe('Mindflayer')
  })
})
