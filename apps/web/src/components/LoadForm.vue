<script setup lang="ts">
import { computed, ref } from 'vue'
import { useRunner } from '@/api/commands'

const runner = useRunner()
const { busy } = runner

/**
 * The workspace to load, as typed. Whether a workspace is there, and
 * whether it is this one, which a load refuses, is the line's to say once
 * it runs: checked nowhere here, so the panel cannot let through a path the
 * CLI would refuse, or refuse one it takes.
 */
const path = ref('')

/**
 * What is sent: the path without the white space around it, which the TUI's
 * load form takes off too (`load_task`), so the same text typed is the same
 * task on either front end, and nothing when that leaves nothing, as there.
 * The server holds it to the path policy (absolute, normalized, outside
 * every `.mindflayer`) and says why when it is not, before anything runs.
 * Unicode's White_Space, which Rust's `str::trim` takes off, rather than
 * JavaScript's `trim`, which takes U+FEFF off as well and leaves U+0085: a
 * path ending in either would be two tasks.
 */
const typed = computed(() => path.value.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, ''))

async function load() {
  if (typed.value === '') {
    return
  }
  const ran = await runner.ask({ type: 'load', path: typed.value })
  if (ran?.outcome.ok) {
    path.value = ''
  }
}
</script>

<template>
  <form
    class="flex flex-wrap items-end gap-2"
    aria-label="Load a workspace by its path"
    data-testid="load-form"
    @submit.prevent="load"
  >
    <UFormField
      label="Load a workspace by its path"
      description="The absolute path of a folder holding a .mindflayer: the line it runs is shown first."
      name="path"
      class="w-full sm:w-auto"
    >
      <UInput
        v-model="path"
        icon="i-lucide-folder-input"
        placeholder="/home/you/skills-team"
        autocomplete="off"
        spellcheck="false"
        size="sm"
        class="w-full font-mono sm:w-96"
        :disabled="busy"
        data-testid="load-path"
      />
    </UFormField>
    <UButton
      type="submit"
      label="Load"
      icon="i-lucide-layers"
      size="sm"
      :disabled="busy || typed === ''"
      data-testid="load-typed"
    />
  </form>
</template>
