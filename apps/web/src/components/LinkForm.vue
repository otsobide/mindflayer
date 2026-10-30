<script setup lang="ts">
import { ref } from 'vue'
import { useRunner } from '@/api/commands'

const runner = useRunner()
const { busy } = runner

/**
 * The folder to link, as typed: sent to the server as it is, which holds it
 * to the path policy (absolute, normalized, outside every `.mindflayer`) and
 * says why when it is not, before anything runs. Checked nowhere here, so
 * the panel cannot let through a path the CLI would refuse, or refuse one
 * it takes.
 */
const path = ref('')

async function link() {
  const ran = await runner.ask({ type: 'link', path: path.value })
  if (ran?.outcome.ok) {
    path.value = ''
  }
}
</script>

<template>
  <form
    class="flex flex-wrap items-end gap-2"
    aria-label="Link a project"
    data-testid="link-form"
    @submit.prevent="link"
  >
    <UFormField
      label="Link a project"
      description="The absolute path of a folder holding a .mind: the line it runs is shown first."
      name="path"
      class="w-full sm:w-auto"
    >
      <UInput
        v-model="path"
        icon="i-lucide-folder-input"
        placeholder="/home/you/Projects/blog"
        autocomplete="off"
        spellcheck="false"
        size="sm"
        class="w-full font-mono sm:w-96"
        :disabled="busy"
        data-testid="link-path"
      />
    </UFormField>
    <UButton
      type="submit"
      label="Link"
      icon="i-lucide-link"
      size="sm"
      :disabled="busy || path === ''"
      data-testid="link"
    />
  </form>
</template>
