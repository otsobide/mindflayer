<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import type { Kind } from '@/api/generated/Kind'
import { useRunner } from '@/api/commands'
import type { Task } from '@/api/transport'
import { libraryLink } from '@/routes'

const props = defineProps<{
  /** The library's own to remove, by its kind and name. */
  kind: Kind
  name: string
}>()

const runner = useRunner()
const { busy } = runner
const router = useRouter()

/** The artifact as a command line names it, which tells one dry run from another's. */
const reference = computed(() => `${props.kind}:${props.name}`)

/**
 * What the dry run said, once it ran: each project's copy, what removing
 * it everywhere does with it and how it stands, and the library's copy, to
 * read before saying yes. Of the artifact it was asked of, and let go once
 * the page shows another.
 */
const dry = ref<{ of: string, said: string }>()

watch(reference, () => {
  dry.value = undefined
})

/**
 * `flayer remove --everywhere`, told `--yes` or not: the workspace's own,
 * taken out of every project it links first, which is the only way the
 * panel removes one, since the library's copy alone would leave each
 * installed copy orphaned.
 */
function removal(yes: boolean): Task {
  return { type: 'remove', holder: { type: 'workspace' }, kind: props.kind, name: props.name, everywhere: true, yes }
}

/**
 * The dry run: a task like any other, shown before it runs, which deletes
 * nothing and says what would go. What it said is shown whenever it ran,
 * a warning beside it included: an unreadable project is said there, and
 * the rest is no less worth reading for it.
 */
async function look() {
  const of = reference.value
  const ran = await runner.ask(removal(false))
  if (ran && ran.outcome.runs.every(run => run.error === null)) {
    dry.value = { of, said: ran.outcome.runs.map(run => run.stdout).join('') }
  }
}

/**
 * The removal itself, offered only once its dry run has been read, which
 * the server's preview makes a confirmation; once it went, back to the
 * library's page of its kind, since this page names one it no longer holds.
 */
async function remove() {
  const kind = props.kind
  const ran = await runner.ask(removal(true))
  if (ran) {
    dry.value = undefined
  }
  if (ran?.outcome.ok) {
    await router.push(libraryLink(kind))
  }
}
</script>

<template>
  <section
    aria-labelledby="remove-heading"
    class="flex flex-col items-start gap-3 rounded-lg border border-default p-4"
    data-testid="remove"
  >
    <h2
      id="remove-heading"
      class="text-sm font-medium text-highlighted"
    >
      Remove it
    </h2>
    <p class="text-sm text-muted">
      Removing it from the library takes it out of every project holding a copy first, as <code class="font-mono text-xs">flayer remove --everywhere</code> does,
      but for a copy changed since it was installed, and anything of its name a project keeps on its own.
      See what that would do first: nothing is deleted until you remove it.
    </p>
    <UButton
      label="See what removing it everywhere does"
      icon="i-lucide-file-search"
      color="neutral"
      variant="outline"
      size="sm"
      :disabled="busy"
      data-testid="remove-dry"
      @click="look"
    />
    <template v-if="dry && dry.of === reference">
      <pre
        class="w-full overflow-x-auto rounded-md border border-default bg-muted px-3 py-2 font-mono text-xs whitespace-pre-wrap text-toned"
        aria-label="What removing it everywhere would do"
        data-testid="remove-said"
      >{{ dry.said }}</pre>
      <UButton
        label="Remove everywhere"
        icon="i-lucide-trash"
        color="error"
        size="sm"
        :disabled="busy"
        data-testid="remove-yes"
        @click="remove"
      />
    </template>
  </section>
</template>
