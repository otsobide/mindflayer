<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { refDebounced } from '@vueuse/core'
import type { Draft } from '@/api/generated/Draft'
import type { Holder } from '@/api/generated/Holder'
import type { Kind } from '@/api/generated/Kind'
import type { Snapshot } from '@/api/generated/Snapshot'
import type { Template } from '@/api/generated/Template'
import { useRunner } from '@/api/commands'
import { useJudged } from '@/api/draft'
import { useSnapshot } from '@/api/snapshot'
import { first } from '@/address'
import AppHeader from '@/components/AppHeader.vue'
import DraftVerdict from '@/components/DraftVerdict.vue'
import SnapshotGate from '@/components/SnapshotGate.vue'
import { artifactLink, libraryLink, newLink } from '@/routes'

const route = useRoute()
const router = useRouter()

// The kind to add is the address's, as the library's page of it is:
// `?kind=rule`. None named is the first the snapshot lists.
const asked = computed(() => first(route.query.kind))

const { data } = useSnapshot()

/** The kind on screen, when the snapshot knows it. */
function shown(snapshot: Snapshot): Kind | undefined {
  const kind = asked.value ?? snapshot.kinds[0]
  return snapshot.kinds.find(known => known === kind)
}

const kind = computed(() => (data.value ? shown(data.value) : undefined))

/** The workspace's own: what is added to. */
const WORKSPACE: Holder = { type: 'workspace' }

const name = ref('')
const description = ref('')

/**
 * The template chosen, by a value of the radio group's own: `none`, or
 * `template:` and its name, so no template's name is taken for none.
 */
const chosen = ref('none')
const template = computed(() => (chosen.value.startsWith('template:') ? chosen.value.slice('template:'.length) : null))

/** The templates of the kind on screen, in the snapshot's order. */
function offered(snapshot: Snapshot, kind: Kind): Template[] {
  return snapshot.library.templates.filter(each => each.kind === kind)
}

// A template is one kind's: another kind starts from none.
watch(kind, () => {
  chosen.value = 'none'
})

const draft = computed<Draft | undefined>(() =>
  kind.value && (name.value !== '' || description.value !== '')
    ? { type: 'add', kind: kind.value, name: name.value, description: description.value, template: template.value }
    : undefined
)
// Judged once somebody stops typing for a moment, not at every key.
const settled = refDebounced(draft, 250)
const judged = useJudged(WORKSPACE, settled)
const judging = computed(() => judged.isPlaceholderData.value || JSON.stringify(settled.value) !== JSON.stringify(draft.value))

const runner = useRunner()
const { busy } = runner

/**
 * Add it, `flayer add`, as the server previews it, then open it. Sent
 * whatever core said of it, which the line then says again, in its words,
 * when it refuses.
 */
async function add() {
  const at = kind.value
  if (!at) {
    return
  }
  const added = name.value
  const ran = await runner.ask({
    type: 'add',
    holder: WORKSPACE,
    kind: at,
    name: added,
    description: description.value,
    template: template.value
  })
  if (ran?.outcome.ok) {
    await router.push(artifactLink(at, added))
  }
}
</script>

<template>
  <UDashboardPanel
    id="new"
    role="main"
    data-testid="new"
  >
    <template #header>
      <AppHeader
        :title="kind ? `New ${kind}` : 'New'"
        icon="i-lucide-file-plus-2"
      />
    </template>

    <template #body>
      <SnapshotGate v-slot="{ snapshot }">
        <UBreadcrumb
          :items="[
            { label: 'Library', icon: 'i-lucide-library-big', to: kind ? libraryLink(kind) : '/library' },
            { label: kind ? `New ${kind}` : 'New' }
          ]"
        />

        <nav
          aria-label="What kind to add"
          data-testid="kinds"
        >
          <UFieldGroup size="sm">
            <UButton
              v-for="each in snapshot.kinds"
              :key="each"
              :to="newLink(each)"
              color="neutral"
              :variant="kind === each ? 'subtle' : 'outline'"
              :aria-current="kind === each ? 'page' : undefined"
              class="font-mono"
              :data-testid="`kind-${each}`"
            >
              {{ each }}
            </UButton>
          </UFieldGroup>
        </nav>

        <p
          v-if="!kind"
          class="text-muted"
          data-testid="no-kind"
        >
          This build of Mindflayer knows no kind called <code class="font-mono">{{ asked }}</code>.
        </p>

        <div
          v-else
          class="grid gap-6 lg:grid-cols-[minmax(0,32rem)_20rem]"
        >
          <form
            class="flex flex-col gap-4"
            aria-label="The new one"
            data-testid="new-form"
            @submit.prevent="add"
          >
            <p class="text-sm text-muted">
              One of the workspace's own, to install into projects: written by <code class="font-mono text-xs">flayer add</code>,
              which shows the line before it runs and judges it as you type.
            </p>

            <UFormField
              label="Name"
              description="What it is called, as a command line names it."
              name="name"
              required
            >
              <UInput
                v-model="name"
                autocomplete="off"
                spellcheck="false"
                size="sm"
                class="w-full font-mono"
                :disabled="busy"
                data-testid="name"
              />
            </UFormField>

            <UFormField
              label="Description"
              description="One line saying what it is, which every listing shows."
              name="description"
              required
            >
              <UInput
                v-model="description"
                autocomplete="off"
                size="sm"
                class="w-full"
                :disabled="busy"
                data-testid="description"
              />
            </UFormField>

            <URadioGroup
              v-model="chosen"
              legend="Start from"
              :items="[
                { value: 'none', label: 'nothing', description: 'the least that passes validate' },
                ...offered(snapshot, kind).map(each => ({ value: `template:${each.name}`, label: each.name, description: each.origin ?? 'built in' }))
              ]"
              :disabled="busy"
              :ui="{ legend: 'mb-1.5 text-sm font-medium text-default', label: 'font-mono text-sm', description: 'break-all font-mono text-xs' }"
              data-testid="template"
            />

            <UButton
              type="submit"
              :label="`Add the ${kind}`"
              icon="i-lucide-plus"
              size="sm"
              class="self-start"
              :disabled="busy || name === ''"
              data-testid="add"
            />
          </form>

          <DraftVerdict
            :judged="settled ? judged.data.value : undefined"
            :pending="judging"
            :error="judged.error.value"
            writer="flayer add"
          />
        </div>
      </SnapshotGate>
    </template>
  </UDashboardPanel>
</template>
