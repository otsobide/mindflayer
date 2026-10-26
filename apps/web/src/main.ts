import './assets/css/main.css'

import { createApp } from 'vue'
import App from './App.vue'
import { httpTransport } from './api/http'
import { createPanel } from './panel'

createApp(App)
  .use(createPanel({ transport: httpTransport() }))
  .mount('#app')
