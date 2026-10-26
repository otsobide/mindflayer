import './assets/css/main.css'

import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'
import { routes } from 'vue-router/auto-routes'
import ui from '@nuxt/ui/vue-plugin'
import App from './App.vue'

createApp(App)
  .use(createRouter({ history: createWebHistory(), routes }))
  .use(ui)
  .mount('#app')
