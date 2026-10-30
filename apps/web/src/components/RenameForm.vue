<script setup lang="ts">
import { ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import type { Kind } from '@/api/generated/Kind'
import { useRunner } from '@/api/commands'
import { artifactLink } from '@/routes'

const props = defineProps<{
  /** The library's own to rename, by its kind and name. */
  kind: Kind
  name: string
}>()

const runner = useRunner()
const { busy } = runner
const router = useRouter()

/** The name it is to have, as typed: the command judges it, and says why it will not. */
const to = ref(props.name)

watch(() => props.name, (name) => {
  to.value = name
})

/**
 * `flayer rename`, shown before it runs, as every task is; once it ran,
 * the page of the name it has now, since this one names a name it no
 * longer has.
 */
async function rename() {
  const kind = props.kind
  const renamed = to.value
  const ran = await runner.ask({ type: 'rename', holder: { type: 'workspace' }, kind, from: props.name, to: renamed })
  if (ran?.outcome.ok) {
    await router.push(artifactLink(kind, renamed))
  }
}
</script>

<template>
  <form
    class="flex flex-col items-start gap-3 rounded-lg border border-default p-4"
    aria-labelledby="rename-heading"
    data-testid="rename"
    @submit.prevent="rename"
  >
    <h2
      id="rename-heading"
      class="text-sm font-medium text-highlighted"
    >
      Rename it
    </h2>
    <p class="text-sm text-muted">
      Its folder or file and the name it declares change together, as <code class="font-mono text-xs">flayer rename</code> changes them:
      the line is shown before it runs, and says why when it will not.
    </p>
    <div class="flex w-full flex-wrap items-end gap-2">
      <UFormField
        label="New name"
        name="to"
        class="w-full sm:w-auto"
      >
        <UInput
          v-model="to"
          autocomplete="off"
          spellcheck="false"
          size="sm"
          class="w-full font-mono sm:w-72"
          :disabled="busy"
          data-testid="rename-to"
        />
      </UFormField>
      <UButton
        type="submit"
        label="Rename"
        icon="i-lucide-text-cursor-input"
        color="neutral"
        variant="outline"
        size="sm"
        :disabled="busy || to === '' || to === name"
        data-testid="rename-run"
      />
    </div>
  </form>
</template>
