import './assets/css/main.css'

import { createApp } from 'vue'
import App from './App.vue'
import { createPanel } from './panel'

createApp(App)
  .use(createPanel())
  .mount('#app')
