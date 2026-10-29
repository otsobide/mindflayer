import pluginVue from 'eslint-plugin-vue'
import stylistic from '@stylistic/eslint-plugin'
import { vueTsConfigs, withVueTs } from '@vue/eslint-config-typescript'

/**
 * The one file that talks to the server.
 *
 * Everything else asks the transport, so authentication lives in one place
 * and a desktop shell can swap HTTP out without touching a view (D-17).
 */
const TRANSPORT = 'src/api/http.ts'

const message = `Ask the transport (useTransport) instead: only ${TRANSPORT} talks to the server`

/** What a browser reaches another machine, or this one, with. */
const NETWORK = ['fetch', 'XMLHttpRequest', 'EventSource', 'WebSocket']

export default withVueTs(
  {
    ignores: [
      'dist/**',
      'node_modules/**',
      // Written by the Vite plugins; checked by vue-tsc, not by hand.
      'src/auto-imports.d.ts',
      'src/components.d.ts',
      'src/route-map.d.ts',
      // Written from Rust by ts-rs (make web/types), in its own style;
      // checked by vue-tsc and against Rust, never edited by hand.
      'src/api/generated/**'
    ]
  },
  pluginVue.configs['flat/recommended'],
  vueTsConfigs.recommended,
  // Nuxt UI's own style, so code copied from its documentation fits.
  stylistic.configs.customize({
    quotes: 'single',
    semi: false,
    commaDangle: 'never',
    braceStyle: '1tbs',
    arrowParens: false
  }),
  {
    rules: {
      'no-restricted-globals': ['error', ...NETWORK.map(name => ({ name, message }))],
      'no-restricted-properties': [
        'error',
        ...['window', 'globalThis', 'self'].flatMap(object =>
          NETWORK.map(property => ({ object, property, message }))
        ),
        { object: 'navigator', property: 'sendBeacon', message }
      ]
    }
  },
  {
    files: [TRANSPORT],
    rules: {
      'no-restricted-globals': 'off',
      'no-restricted-properties': 'off'
    }
  },
  {
    // A page is named for its route: `index`, `[...path]`.
    files: ['src/pages/**/*.vue'],
    rules: {
      'vue/multi-word-component-names': 'off'
    }
  }
)
