<script setup lang="ts">
import { ref } from 'vue'
import { useRunner } from '@/api/commands'
import FolderPicker from '@/components/FolderPicker.vue'

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

/** Whether the explorer is open, to choose the folder rather than type it. */
const browsing = ref(false)

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
    <!--
      Opened where the field says, or at the workspace's root when it says
      nothing; the folder chosen goes in the field, to be linked as a typed
      one is.
    -->
    <UButton
      type="button"
      label="Browse"
      icon="i-lucide-folder-search"
      size="sm"
      color="neutral"
      variant="outline"
      :disabled="busy"
      data-testid="link-browse"
      @click="browsing = true"
    />
    <UButton
      type="submit"
      label="Link"
      icon="i-lucide-link"
      size="sm"
      :disabled="busy || path === ''"
      data-testid="link"
    />
    <FolderPicker
      v-model:open="browsing"
      :start="path"
      title="Choose a folder to link"
      verb="Choose"
      @chosen="path = $event"
    />
  </form>
</template>
