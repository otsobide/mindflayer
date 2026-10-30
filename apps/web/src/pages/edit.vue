<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { watchDebounced } from '@vueuse/core'
import type { Draft } from '@/api/generated/Draft'
import type { Held } from '@/api/generated/Held'
import type { Holder } from '@/api/generated/Holder'
import { useContents, useFiles, type Named } from '@/api/artifact'
import { useRunner } from '@/api/commands'
import { useJudged } from '@/api/draft'
import { useSnapshot } from '@/api/snapshot'
import { first } from '@/address'
import AppHeader from '@/components/AppHeader.vue'
import DraftVerdict from '@/components/DraftVerdict.vue'
import MarkdownEditor from '@/components/MarkdownEditor.vue'
import SnapshotGate from '@/components/SnapshotGate.vue'
import { artifactLink, libraryLink } from '@/routes'

const route = useRoute()

// Which of the library's own is edited is the address's, as on its page:
// `?kind=skill&name=review`.
const kind = computed(() => first(route.query.kind))
const name = computed(() => first(route.query.name))

const { data } = useSnapshot()

/** The one the address names, as the snapshot lists it. */
const held = computed<Held | undefined>(() =>
  data.value?.library.artifacts.find(each => each.kind === kind.value && each.name === name.value)
)

/** The workspace's own: what every read and the save are of. */
const WORKSPACE: Holder = { type: 'workspace' }

const named = computed<Named | undefined>(() =>
  held.value ? { holder: WORKSPACE, reference: `${held.value.kind}:${held.value.name}` } : undefined
)

const files = useFiles(named)
/** The file it is read from, its `SKILL.md` or a rule's own, as its listing names it. */
const manifest = computed(() => files.data.value?.manifest)
const contents = useContents(named, manifest)

/**
 * What the text in the editor was started from: what the file held, and the
 * SHA-256 core gave of it, which a save is held to (`--if-unchanged`), so a
 * change made on disk since is not written over.
 */
interface Read {
  text: string
  hash: string
}

const read = ref<Read>()
/** The text as it is in the editor now. */
const text = ref('')
/**
 * The text core judges: the editor's once somebody stops typing for a
 * moment, not at every key, and what was read at once.
 */
const settled = ref('')
watchDebounced(text, (now) => {
  settled.value = now
}, { debounce: 250 })

// What the file holds, each time it is read: taken into the editor while
// nothing typed there would be lost, or once the file holds what the editor
// does, as after a save. Otherwise what was read stays what a save is held
// to, and a save is refused for the change, with core's words.
watch(() => contents.data.value, (now) => {
  if (typeof now?.text !== 'string' || now.hash === null) {
    return
  }
  const fresh = { text: now.text, hash: now.hash }
  if (read.value === undefined || text.value === read.value.text || text.value === fresh.text) {
    read.value = fresh
    text.value = fresh.text
    settled.value = fresh.text
  }
}, { immediate: true })

const dirty = computed(() => read.value !== undefined && text.value !== read.value.text)

/** Whether the file holds something else now than the text was started from. */
const changedOnDisk = computed(() => {
  const now = contents.data.value
  return dirty.value && typeof now?.hash === 'string' && now.hash !== read.value?.hash
})

/** Start again from what the file holds now, what was typed let go. */
function readAgain() {
  const now = contents.data.value
  if (typeof now?.text === 'string' && now.hash !== null) {
    read.value = { text: now.text, hash: now.hash }
    text.value = now.text
    settled.value = now.text
  }
}

const draft = computed<Draft | undefined>(() =>
  held.value && read.value ? { type: 'edit', reference: `${held.value.kind}:${held.value.name}`, text: settled.value } : undefined
)
const judged = useJudged(WORKSPACE, draft)
/** Whether what is drawn is of the text as it is now. */
const judging = computed(() => judged.isPlaceholderData.value || settled.value !== text.value)

const runner = useRunner()
const { busy } = runner

/**
 * Save it: the text and the hash it was started from, sent as a task the
 * server previews as the line it runs, `flayer edit --contents`, and runs
 * once asked. Never refused here for what core says of it: the line is
 * core's to refuse, in its words, as it refuses a change made on disk.
 */
async function save() {
  if (!held.value || !read.value || !dirty.value || busy.value) {
    return
  }
  await runner.ask({
    type: 'save',
    holder: WORKSPACE,
    kind: held.value.kind,
    name: held.value.name,
    text: text.value,
    hash: read.value.hash
  })
}

const title = computed(() => `Edit ${held.value?.name ?? name.value ?? ''}`.trim())

/** Why its text cannot be edited here, when it cannot. */
const withheld = computed(() => {
  const now = contents.data.value
  if (!now || typeof now.text === 'string') {
    return undefined
  }
  return now.withheld === 'binary'
    ? `It is not text: ${now.size} bytes, which an editor of text would change.`
    : `It holds ${now.size} bytes, more than the panel reads.`
})
</script>

<template>
  <UDashboardPanel
    id="edit"
    role="main"
    data-testid="edit"
  >
    <template #header>
      <AppHeader
        :title="title"
        icon="i-lucide-file-pen-line"
      />
    </template>

    <template #body>
      <SnapshotGate v-slot="{ snapshot }">
        <UBreadcrumb
          :items="[
            { label: 'Library', icon: 'i-lucide-library-big', to: held ? libraryLink(held.kind) : '/library' },
            ...(held ? [{ label: held.name, to: artifactLink(held.kind, held.name) }] : []),
            { label: 'Edit' }
          ]"
        />

        <div
          v-if="!held"
          class="flex flex-col items-start gap-3"
          data-testid="no-artifact"
        >
          <p class="text-muted">
            <template v-if="kind === undefined || name === undefined">
              Nothing is named to edit: the address says no kind or no name.
            </template>
            <template v-else>
              {{ snapshot.workspace.name }} keeps no {{ kind }} of its own called <code class="font-mono">{{ name }}</code>.
            </template>
          </p>
          <UButton
            label="The library"
            icon="i-lucide-arrow-left"
            to="/library"
            color="neutral"
            variant="outline"
            size="sm"
          />
        </div>

        <template v-else>
          <div class="flex flex-col gap-1">
            <div class="flex flex-wrap items-center gap-2">
              <h2
                class="font-mono text-base font-semibold text-highlighted"
                data-testid="name"
              >
                {{ held.name }}
              </h2>
              <UBadge
                :label="held.kind"
                color="neutral"
                variant="outline"
                size="sm"
                class="font-mono"
              />
              <code
                v-if="manifest"
                class="font-mono text-xs text-muted"
                data-testid="file"
              >{{ manifest }}</code>
              <UBadge
                v-if="dirty"
                label="unsaved"
                icon="i-lucide-pencil"
                color="warning"
                variant="subtle"
                size="sm"
                data-testid="unsaved"
              />
            </div>
            <p class="text-sm text-muted">
              Saving runs <code class="font-mono text-xs">flayer edit --contents</code> with the text, held to what it held when it was read,
              so a change made to it since is never written over. Ctrl-S saves from the editor; Tab leaves it.
            </p>
          </div>

          <UAlert
            v-if="changedOnDisk"
            color="warning"
            variant="subtle"
            icon="i-lucide-file-diff"
            title="It changed on disk since it was read into the editor"
            description="Saving is held to what it held then. Read it again to start from what it holds now, letting go of what you typed."
            data-testid="changed"
          >
            <template #actions>
              <UButton
                label="Read it again"
                icon="i-lucide-refresh-cw"
                color="neutral"
                variant="outline"
                size="xs"
                data-testid="read-again-changed"
                @click="readAgain"
              />
            </template>
          </UAlert>

          <p
            v-if="withheld"
            class="text-sm text-muted"
            data-testid="withheld"
          >
            {{ withheld }}
          </p>
          <UAlert
            v-else-if="files.error.value ?? contents.error.value"
            color="warning"
            variant="subtle"
            icon="i-lucide-file-x"
            title="Its file could not be read"
            :description="(files.error.value ?? contents.error.value)?.message"
            data-testid="unread"
          />
          <p
            v-else-if="!read"
            role="status"
            class="text-xs text-muted"
          >
            Reading its file
          </p>

          <div
            v-else
            class="grid gap-4 lg:grid-cols-[minmax(0,1fr)_20rem]"
          >
            <MarkdownEditor
              v-model="text"
              :label="`The text of ${held.name}'s ${manifest ?? 'file'}`"
              @save="save"
            />
            <div class="flex flex-col gap-4">
              <div class="flex flex-wrap gap-2">
                <UButton
                  label="Save"
                  icon="i-lucide-save"
                  size="sm"
                  :disabled="!dirty || busy"
                  data-testid="save"
                  @click="save"
                />
                <UButton
                  label="Read it again"
                  icon="i-lucide-undo-2"
                  color="neutral"
                  variant="outline"
                  size="sm"
                  :disabled="!dirty || busy"
                  data-testid="read-again"
                  @click="readAgain"
                />
                <UButton
                  label="Close"
                  icon="i-lucide-x"
                  color="neutral"
                  variant="ghost"
                  size="sm"
                  :to="artifactLink(held.kind, held.name)"
                  data-testid="close"
                />
              </div>
              <DraftVerdict
                :judged="judged.data.value"
                :pending="judging"
                :error="judged.error.value"
                writer="flayer edit"
              />
            </div>
          </div>
        </template>
      </SnapshotGate>
    </template>
  </UDashboardPanel>
</template>
