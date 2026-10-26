// @vitest-environment node
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import { ESLint } from 'eslint'

const root = fileURLToPath(new URL('..', import.meta.url))
const eslint = new ESLint({ cwd: root })

/** The rules `code` breaks, linted as though it were the file at `path`. */
async function broken(path: string, code: string): Promise<string[]> {
  const [result] = await eslint.lintText(code, { filePath: path })
  return (result?.messages ?? []).map(message => message.ruleId ?? message.message)
}

describe('the rule that keeps the network in the transport', () => {
  it('refuses fetch anywhere but the transport', async () => {
    expect(await broken('src/pages/sneaky.ts', 'export const answer = fetch(\'/api/query\')\n'))
      .toEqual(['no-restricted-globals'])
  })

  it('refuses it reached through window or globalThis', async () => {
    expect(await broken('src/stores/sneaky.ts', 'export const answer = window.fetch(\'/api/query\')\n'))
      .toEqual(['no-restricted-properties'])
    expect(await broken('src/stores/sneaky.ts', 'export const answer = globalThis.fetch(\'/api/query\')\n'))
      .toEqual(['no-restricted-properties'])
  })

  it('refuses the other ways a page talks to a server', async () => {
    for (const code of ['new EventSource(\'/api/events\')', 'new WebSocket(\'ws://127.0.0.1\')', 'new XMLHttpRequest()']) {
      expect(await broken('src/components/Sneaky.ts', `export const channel = ${code}\n`), code)
        .toEqual(['no-restricted-globals'])
    }
  })

  it('refuses fetch in a component as well', async () => {
    const component = [
      '<script setup lang="ts">',
      'const answer = fetch(\'/api/query\')',
      '</script>',
      '',
      '<template>',
      '  <p>{{ answer }}</p>',
      '</template>',
      ''
    ].join('\n')

    expect(await broken('src/components/SneakyView.vue', component)).toEqual(['no-restricted-globals'])
  })

  it('lets the transport use it', async () => {
    expect(await broken('src/api/http.ts', 'export const answer = fetch(\'/api/query\')\n')).toEqual([])
  })
})
