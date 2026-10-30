<script setup lang="ts">
import { ref, watch } from 'vue'
import type { MarkerFile } from '@/api/generated/MarkerFile'
import { useRunner } from '@/api/commands'

const props = defineProps<{
  /** The project's entry, which the server turns into its folder. */
  entry: string
  /** The older marker it is read from, as the snapshot says. */
  marker: MarkerFile | null
}>()

const runner = useRunner()
const { busy } = runner

/**
 * What the dry run said, once it ran: the lines a migration would write,
 * move and leave, to read before saying yes. Of the entry it was asked of,
 * and let go once the page shows another, or the migration ran.
 */
const dry = ref<{ entry: string, said: string }>()

watch(() => props.entry, () => {
  dry.value = undefined
})

/**
 * The dry run, `flayer migrate -p` without `--yes`: a task like any other,
 * shown before it runs, which writes nothing and says what would move.
 */
async function look() {
  const entry = props.entry
  const ran = await runner.ask({ type: 'migrate', project: entry, yes: false })
  if (ran?.outcome.ok) {
    dry.value = { entry, said: ran.outcome.runs.map(run => run.stdout).join('') }
  }
}

/** The migration itself, offered only once its dry run has been read. */
async function migrate() {
  const ran = await runner.ask({ type: 'migrate', project: props.entry, yes: true })
  if (ran?.outcome.ok) {
    dry.value = undefined
  }
}
</script>

<template>
  <section
    aria-labelledby="migrate-heading"
    class="flex flex-col items-start gap-3 rounded-lg border border-default p-4"
    data-testid="migrate"
  >
    <h2
      id="migrate-heading"
      class="text-sm font-medium text-highlighted"
    >
      Marked by .mind/{{ marker }}, which an older Mindflayer wrote
    </h2>
    <p class="text-sm text-muted">
      Its agents and share mode are changed by nothing until <code class="font-mono text-xs">mind migrate</code> moves it to a settings.toml, carrying what was installed into it to where its agents read.
      See what that would do first: nothing changes until you migrate.
    </p>
    <UButton
      label="See what migrating does"
      icon="i-lucide-file-search"
      color="neutral"
      variant="outline"
      size="sm"
      :disabled="busy"
      data-testid="migrate-dry"
      @click="look"
    />
    <template v-if="dry && dry.entry === entry">
      <pre
        class="w-full overflow-x-auto rounded-md border border-default bg-muted px-3 py-2 font-mono text-xs whitespace-pre-wrap text-toned"
        aria-label="What migrating would do"
        data-testid="migrate-said"
      >{{ dry.said }}</pre>
      <UButton
        label="Migrate"
        icon="i-lucide-arrow-big-up-dash"
        size="sm"
        :disabled="busy"
        data-testid="migrate-yes"
        @click="migrate"
      />
    </template>
  </section>
</template>
