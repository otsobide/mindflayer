/**
 * The panel's palette: which Tailwind colour each of Nuxt UI's colour names
 * stands for.
 *
 * Zinc for everything neutral and violet as the one accent; the other three
 * say what state something is in, and are never decoration. Nuxt UI's
 * `secondary` is left out, so a second accent cannot creep in.
 *
 * main.css spells the same mapping out as CSS variables, because the plugin
 * that would write them from this object at run time writes a `<style>`
 * element the server's CSP refuses (see src/overrides/colors.ts). The build
 * test holds the two to each other.
 */
export const colors = {
  primary: 'violet',
  info: 'sky',
  success: 'emerald',
  warning: 'amber',
  error: 'red',
  neutral: 'zinc'
} as const

/** The shades Nuxt UI derives a colour's variables from. */
export const shades = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950] as const
