// @vitest-environment node
import { existsSync, lstatSync, readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

const root = fileURLToPath(new URL('..', import.meta.url))

/**
 * Licences whose terms a GPL-3.0-only program can take in: the permissive
 * ones, the font licence, and MPL-2.0, which allows the GPL as a secondary
 * licence unless a file says otherwise.
 */
const COMPATIBLE = new Set([
  '0BSD',
  'Apache-2.0',
  'BlueOak-1.0.0',
  'BSD-2-Clause',
  'BSD-3-Clause',
  'CC-BY-4.0',
  'CC0-1.0',
  'ISC',
  'MIT',
  'MPL-2.0',
  'OFL-1.1',
  'Unlicense'
])

/**
 * Packages that do not say their licence, and what their source says.
 * Each is checked by hand, and named here so that a new one fails.
 */
const UNDECLARED: Record<string, string> = {
  // Its package.json has no `license`, and the package ships no LICENSE;
  // the repository it is published from is MIT.
  'vaul-vue': 'MIT'
}

interface Installed {
  name: string
  version: string
  licence: string | undefined
}

/** A package.json's licence, in the shapes npm has accepted over the years. */
function licenceOf(manifest: Record<string, unknown>): string | undefined {
  const { license, licenses } = manifest
  if (typeof license === 'string') {
    return license
  }
  if (typeof license === 'object' && license !== null && 'type' in license) {
    return String(license.type)
  }
  if (Array.isArray(licenses)) {
    return licenses.map(each => (typeof each === 'object' && each !== null && 'type' in each ? String(each.type) : String(each))).join(' OR ')
  }
  return undefined
}

/**
 * Every package pnpm installed, development tools included: in its store
 * under node_modules/.pnpm each one is a real folder, beside links to what
 * it depends on.
 */
function installed(): Installed[] {
  const store = join(root, 'node_modules', '.pnpm')
  const packages: Installed[] = []
  for (const entry of readdirSync(store)) {
    const modules = join(store, entry, 'node_modules')
    if (!existsSync(modules)) {
      continue
    }
    for (const name of readdirSync(modules)) {
      const folders = name.startsWith('@')
        ? readdirSync(join(modules, name)).map(inner => `${name}/${inner}`)
        : [name]
      for (const folder of folders) {
        const path = join(modules, folder)
        if (lstatSync(path).isSymbolicLink() || !existsSync(join(path, 'package.json'))) {
          continue
        }
        const manifest = JSON.parse(readFileSync(join(path, 'package.json'), 'utf8')) as Record<string, unknown>
        packages.push({
          name: String(manifest.name ?? folder),
          version: String(manifest.version ?? ''),
          licence: licenceOf(manifest)
        })
      }
    }
  }
  return packages
}

/**
 * Whether an SPDX expression lets the package in on at least one of its
 * choices. Read with its grouping, OR the loosest, then AND, then
 * parentheses: with them stripped, `(MIT OR Apache-2.0) AND SSPL-1.0` would
 * read as a choice of MIT alone. `X WITH Y` is one term, let in only if it is
 * listed whole, and an expression that does not parse is refused.
 */
function compatible(expression: string): boolean {
  const tokens = expression.match(/[()]|[^\s()]+/g) ?? []
  let at = 0
  const malformed = () => new Error(`not an SPDX expression: ${expression}`)

  // Each side is read before the two are combined, so that a choice already
  // let in still consumes the rest of the expression.
  function anyOf(): boolean {
    let allowed = allOf()
    while (tokens[at] === 'OR') {
      at += 1
      allowed = allOf() || allowed
    }
    return allowed
  }
  function allOf(): boolean {
    let allowed = term()
    while (tokens[at] === 'AND') {
      at += 1
      allowed = term() && allowed
    }
    return allowed
  }
  function term(): boolean {
    const token = tokens[at++]
    if (token === '(') {
      const allowed = anyOf()
      if (tokens[at++] !== ')') {
        throw malformed()
      }
      return allowed
    }
    if (token === undefined || [')', 'AND', 'OR', 'WITH'].includes(token)) {
      throw malformed()
    }
    if (tokens[at] === 'WITH') {
      const exception = tokens[at + 1]
      at += 2
      if (exception === undefined || [')', '(', 'AND', 'OR', 'WITH'].includes(exception)) {
        throw malformed()
      }
      return COMPATIBLE.has(`${token} WITH ${exception}`)
    }
    return COMPATIBLE.has(token)
  }

  try {
    const allowed = anyOf()
    return at === tokens.length && allowed
  } catch {
    return false
  }
}

describe('reading a licence expression', () => {
  it.each([
    ['MIT', true],
    ['MIT OR GPL-2.0-only', true],
    ['(MIT OR Apache-2.0) AND BSD-3-Clause', true],
    ['MIT OR Apache-2.0 AND SSPL-1.0', true],
    ['MIT AND SSPL-1.0', false],
    ['(MIT OR Apache-2.0) AND SSPL-1.0', false],
    ['GPL-2.0-only AND (MIT OR ISC)', false],
    ['CC-BY-NC-4.0 AND (MIT OR Apache-2.0)', false],
    ['Apache-2.0 WITH LLVM-exception', false],
    ['(MIT OR ISC', false],
    ['MIT ISC', false],
    ['', false]
  ])('reads %j as %s', (expression, allowed) => {
    expect(compatible(expression)).toBe(allowed)
  })
})

describe('the licences of what the panel installs', () => {
  const packages = installed()

  it('finds what pnpm installed', () => {
    expect(packages.map(each => each.name)).toEqual(expect.arrayContaining(['vue', '@nuxt/ui', 'vite']))
  })

  it('are each compatible with GPL-3.0-only', () => {
    const refused = packages
      .map(each => ({ ...each, licence: each.licence ?? UNDECLARED[each.name] }))
      .filter(each => each.licence === undefined || !compatible(each.licence))
      .map(each => `${each.name}@${each.version}: ${each.licence ?? 'no licence declared'}`)

    expect(refused).toEqual([])
  })

  it('name every package without a declared licence', () => {
    const undeclared = packages.filter(each => each.licence === undefined).map(each => each.name)

    expect([...new Set(undeclared)].sort()).toEqual(Object.keys(UNDECLARED).sort())
  })
})
