// @vitest-environment node
import { execFileSync } from 'node:child_process'
import { mkdtempSync, readdirSync, readFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { colors, shades } from '@/theme'

const root = fileURLToPath(new URL('..', import.meta.url))

/**
 * What a page may name on another host and still fetch nothing from it:
 * namespace names, which the DOM compares as strings and never requests, and
 * the links Vue prints to the console beside a production error, which end
 * in the error's code.
 */
const NAMES = new Set([
  'http://www.w3.org/2000/svg',
  'http://www.w3.org/1999/xlink',
  'http://www.w3.org/1998/Math/MathML',
  'http://www.w3.org/XML/1998/namespace',
  'http://www.w3.org/2000/xmlns/'
])
const PRINTED = 'https://vuejs.org/error-reference/'

/** A URL with a host: `https://host/...`, `//host/...`, `ws://host`. */
const HOSTED = /(?:\b[a-z][a-z0-9+.-]*:)?\/\/[a-z0-9-]+(?:\.[a-z0-9-]+)+[^\s"'`()<>\\]*/gi

/** A licence banner, `/*! ... *\/`, which minifiers keep and nothing reads. */
const BANNER = /\/\*![\s\S]*?\*\//g

// The panel as `make web/build` builds it, but into a folder of its own, so
// the test needs no build beforehand and leaves dist/ as it found it. In a
// process of its own too: in this one, vitest's NODE_ENV=test would make it
// a development build, and that is not what the server embeds.
let out: string
const files = new Map<string, string>()

beforeAll(() => {
  out = mkdtempSync(join(root, 'node_modules', '.dist-test-'))
  const env = Object.fromEntries(
    Object.entries(process.env).filter(([name]) => name !== 'NODE_ENV' && !name.startsWith('VITEST'))
  )
  execFileSync(
    process.execPath,
    [join(root, 'node_modules', 'vite', 'bin', 'vite.js'), 'build', '--outDir', out, '--emptyOutDir', '--logLevel', 'error'],
    { cwd: root, env, stdio: 'pipe' }
  )
  for (const entry of readdirSync(out, { recursive: true, withFileTypes: true })) {
    if (entry.isFile() && /\.(?:html|js|css)$/.test(entry.name)) {
      const path = join(entry.parentPath, entry.name).slice(out.length + 1).replaceAll('\\', '/')
      files.set(path, readFileSync(join(entry.parentPath, entry.name), 'utf8'))
    }
  }
}, 120_000)

afterAll(() => {
  if (out) {
    rmSync(out, { recursive: true, force: true })
  }
})

function page(): string {
  const html = files.get('index.html')
  expect(html, 'index.html').toBeDefined()
  return html!
}

function sheets(): string[] {
  return [...files].filter(([path]) => path.endsWith('.css')).map(([, text]) => text)
}

describe('the built panel', () => {
  it('names no other host in its page, scripts or styles', () => {
    expect([...files.keys()].some(path => path.endsWith('.js'))).toBe(true)
    for (const [path, text] of files) {
      const hosts = (text.replace(BANNER, '').match(HOSTED) ?? [])
        .filter(found => !NAMES.has(found) && !found.startsWith(PRINTED))
      expect(hosts, path).toEqual([])
    }
  })

  it('loads everything from its own origin, by absolute path', () => {
    const html = page()
    const references = [...html.matchAll(/\b(?:src|href)="([^"]*)"/g)].map(match => match[1])
    expect(references.length).toBeGreaterThan(0)
    for (const reference of references) {
      expect(reference, reference).toMatch(/^\/(?!\/)/)
    }
    for (const sheet of sheets()) {
      for (const [, url] of sheet.matchAll(/url\(\s*["']?([^"')]*)/g)) {
        // Not even data: URLs, which the CSP's default-src 'self' refuses
        // for fonts and images alike.
        expect(url, url).toMatch(/^\/assets\//)
      }
    }
  })

  it('carries no inline script or style for the CSP to refuse', () => {
    const html = page()
    for (const [tag, attributes, body] of html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/g)) {
      expect(attributes, tag).toMatch(/\bsrc="/)
      expect(body, tag).toBe('')
    }
    expect(html).not.toMatch(/<style\b/)
    expect(html).not.toMatch(/\sstyle="/)
    expect(html).not.toMatch(/\son[a-z]+="/)
  })

  it('declares in its stylesheet the palette Nuxt UI would have injected', () => {
    const css = sheets().join('\n')
    for (const [name, color] of Object.entries(colors)) {
      for (const shade of shades) {
        expect(css).toContain(`--ui-color-${name}-${shade}:var(--color-${color}-${shade})`)
        // And Tailwind emitted the colour each one points at.
        expect(css).toMatch(new RegExp(`--color-${color}-${shade}:[^;}]`))
      }
    }
    expect(css).toContain('--ui-primary:var(--ui-color-primary-500)')
    expect(css).toContain('--ui-primary:var(--ui-color-primary-400)')
  })

  it('leaves out the code that would fetch an icon or inject the palette', () => {
    const scripts = [...files].filter(([path]) => path.endsWith('.js')).map(([, text]) => text).join('\n')
    expect(scripts).not.toContain('iconify.design')
    expect(scripts).not.toContain('nuxt-ui-colors')
  })
})
