import { setSSRHandler, unrefElement, useColorMode } from '@vueuse/core'

/** The class main.css stops every transition under, for one switch. */
const SWITCHING = 'switching-color-mode'

/**
 * Dark by default, and switched without a `<style>` element.
 *
 * vueuse's colour mode follows the system when nothing is stored, so dark is
 * stored first, before Nuxt UI's plugin reads it.
 *
 * To switch, vueuse adds a `<style>` that stops transitions while the classes
 * change, from every one of the several places that watch the mode, and a CSP
 * without `'unsafe-inline'` refuses each of them. Its `updateHTMLAttrs`
 * handler is the hook it gives for changing how that is done, so the panel
 * does it with a class main.css already holds instead. It is read when each
 * watcher starts, so this has to run before any of them.
 */
export function installColorMode(): void {
  setSSRHandler('updateHTMLAttrs', (selector, attribute, value) => {
    const element = typeof selector === 'string' ? document.querySelector(selector) : unrefElement(selector)
    if (!(element instanceof Element) || attribute !== 'class') {
      return
    }
    // vueuse's own handler only touches the classes of the modes its
    // watcher was given, and the watchers differ: `useDark` spells light as
    // no class at all, `useColorMode` as `light`. So `dark` follows every
    // value, and `light` is added only by a value that names it and taken
    // away only by one that names `dark`, whichever watcher runs last.
    const classes = element.classList
    const names = value.split(/\s+/)
    const dark = names.includes('dark')
    const light = !dark && names.includes('light')
    const settled = classes.contains('dark') === dark
      && (dark ? !classes.contains('light') : !light || classes.contains('light'))
    if (settled) {
      return
    }
    const root = document.documentElement
    root.classList.add(SWITCHING)
    classes.toggle('dark', dark)
    if (dark) {
      classes.remove('light')
    } else if (light) {
      classes.add('light')
    }
    // Make the browser work out the new styles while transitions are still
    // off, or it would only do so after the class below is gone.
    void getComputedStyle(element).opacity
    root.classList.remove(SWITCHING)
  })
  useColorMode({ initialValue: 'dark' })
}
