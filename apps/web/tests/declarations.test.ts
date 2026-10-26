// @vitest-environment node
import { readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

const src = fileURLToPath(new URL('../src', import.meta.url))

/**
 * The declarations Nuxt UI's plugins write, committed so vue-tsc knows its
 * components and composables on a fresh clone. They name each by its path
 * in pnpm's store, whose folder for @nuxt/ui carries a hash of its peers, so
 * a bump of vue, vue-router or tailwindcss moves every one. A path that
 * names nothing makes that type `any`, and with `@ts-nocheck` and
 * `skipLibCheck` nothing says so: vue-tsc stops checking those props and
 * passes. Hence a test.
 */
const DECLARATIONS = ['components.d.ts', 'auto-imports.d.ts']

/** What TypeScript tries after a path written without its extension. */
const ENDINGS = ['', '.d.ts', '.ts', '.d.mts', '.mts', '.js', '.mjs', '/index.d.ts', '/index.ts', '/index.js']

function isFile(path: string): boolean {
  try {
    return statSync(path).isFile()
  } catch {
    return false
  }
}

/** Every relative path a declaration file imports a type from. */
function named(file: string): string[] {
  const text = readFileSync(join(src, file), 'utf8')
  return [...text.matchAll(/import\(\s*['"](\.{1,2}\/[^'"]+)['"]\s*\)/g)].map(match => match[1]!)
}

describe('the committed declarations', () => {
  it.each(DECLARATIONS)('%s names only files that are installed', (file) => {
    const paths = named(file)
    expect(paths.length).toBeGreaterThan(0)

    const missing = [...new Set(paths)].filter(path => !ENDINGS.some(ending => isFile(join(src, path + ending))))

    expect(missing, `stale: make web/build writes ${file} again`).toEqual([])
  })
})
