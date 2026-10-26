import pluginVue from 'eslint-plugin-vue'
import stylistic from '@stylistic/eslint-plugin'
import { vueTsConfigs, withVueTs } from '@vue/eslint-config-typescript'

export default withVueTs(
  {
    ignores: [
      'dist/**',
      'node_modules/**',
      // Written by the Vite plugins; checked by vue-tsc, not by hand.
      'src/auto-imports.d.ts',
      'src/components.d.ts',
      'src/route-map.d.ts'
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
    // A page is named for its route: `index`, `[...path]`.
    files: ['src/pages/**/*.vue'],
    rules: {
      'vue/multi-word-component-names': 'off'
    }
  }
)
