<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import type { MindsChange } from '@/api/generated/MindsChange'
import type { Suggestion } from '@/api/generated/Suggestion'
import { useRunner } from '@/api/commands'
import { useScan, type Asked } from '@/api/scan'
import { useSnapshot } from '@/api/snapshot'
import { TransportError } from '@/api/transport'
import { every, first, useQueryChange } from '@/address'
import AppHeader from '@/components/AppHeader.vue'
import CommandHint, { flayer } from '@/components/CommandHint.vue'
import SnapshotGate from '@/components/SnapshotGate.vue'

const route = useRoute()
const change = useQueryChange()
const runner = useRunner()
const { busy } = runner

// What was asked is the address's (src/address.ts), `?root=/home/you/Work
// &root=/srv/git&depth=2`, so a reload or a link scans the same folders
// again. No folder named is nothing asked: the page scans only once
// somebody asks, never on its own.
const asked = computed<Asked | undefined>(() => {
  const roots = every(route.query.root)
  const depth = first(route.query.depth)
  return roots.length === 0 ? undefined : { roots, depth: depth === undefined ? null : Number(depth) }
})

const { data: snapshot } = useSnapshot()
const { data: scanned, error, isFetching, refetch } = useScan(asked)

/**
 * The folders as typed, one a line, sent as they are for the server to
 * hold to the path policy, as the folder to link is: absolute, normalized,
 * outside every `.mindflayer`. Checked nowhere here, so the panel cannot
 * let through a folder the CLI would refuse, or refuse one it takes.
 */
const roots = ref('')
/** How many levels down, none for core's own depth. */
const depth = ref<number | null>(null)

// What the address asked, or the workspace's own folder while it asks
// nothing, as `flayer scan` with no folder looks there: a starting point
// to change, never scanned until asked.
watch([asked, () => snapshot.value?.workspace.path], ([now, path]) => {
  if (now) {
    roots.value = now.roots.join('\n')
    depth.value = now.depth
  } else if (roots.value === '' && path) {
    roots.value = path
  }
}, { immediate: true })

/** Scan what the form says, or the same again when it says what was asked. */
function scan() {
  const typed = roots.value.split('\n').filter(line => line !== '')
  const same = JSON.stringify({ roots: typed, depth: depth.value }) === JSON.stringify(asked.value)
  if (same) {
    void refetch()
  } else {
    change({ root: typed, depth: depth.value === null ? undefined : String(depth.value) })
  }
}

// Grouped, never sorted: the scan's order is `flayer scan --suggest`'s,
// core's, what is most worth linking first.
const moved = computed(() => scanned.value?.found.filter(suggestion => suggestion.moved_from !== null) ?? [])
const offered = computed(() => scanned.value?.found.filter(suggestion => suggestion.offers !== null) ?? [])
const linked = computed(() => scanned.value?.found.filter(suggestion => suggestion.found.linked) ?? [])

/** The folders marked to link, by path. */
const marked = ref<string[]>([])

// A new answer is a new list: what was marked in the last may be linked now.
watch(scanned, () => {
  marked.value = []
})

function mark(suggestion: Suggestion, on: boolean | 'indeterminate') {
  const path = suggestion.found.path
  marked.value = on === true ? [...marked.value, path] : marked.value.filter(each => each !== path)
}

/** Those marked, in the scan's order, which the batch keeps. */
const picked = computed(() => offered.value.filter(suggestion => marked.value.includes(suggestion.found.path)))

/** Those marked that `mind init` writes into before they are linked. */
const writtenInto = computed(() => picked.value.filter(suggestion => suggestion.offers === 'init_and_link'))

/**
 * Link what is marked: its marks, each with what the scan said it offers,
 * which the backend turns into the minds screen's batch (`Task::minds`), a
 * `mind init` before the link for a repository that is not a project yet.
 * The lines are shown before they run. The scan is asked again after, since
 * what it found linked has changed, and no event asks for it.
 */
async function link() {
  const marks = picked.value.map(suggestion => ({ path: suggestion.found.path, change: suggestion.offers as MindsChange }))
  const ran = await runner.ask({ type: 'marks', marks })
  if (ran) {
    void refetch()
  }
}

/** Follow an entry to where the scan found its project moved. */
async function relink(suggestion: Suggestion) {
  const ran = await runner.ask({ type: 'relink', old: suggestion.moved_from!, new: suggestion.found.path })
  if (ran) {
    void refetch()
  }
}

// What marking each offers, said by its word: the panel's look, kept for
// each word it knows and plain for one it does not, as a state's is.
const OFFERS: Partial<Record<MindsChange, { label: string, icon: string }>> = {
  link: { label: 'a mind project: linked as it is', icon: 'i-lucide-link' },
  init_and_link: { label: 'not a mind project yet: mind init makes it one, then it is linked', icon: 'i-lucide-folder-plus' }
}

/** Why a scan was refused, with its code. */
const refusal = computed(() => {
  const failed = error.value
  if (!failed) {
    return undefined
  }
  const code = failed instanceof TransportError ? failed.code : undefined
  return code ? `${failed.message} (${code})` : failed.message
})
</script>

<template>
  <UDashboardPanel
    id="discover"
    role="main"
    data-testid="discover"
  >
    <template #header>
      <AppHeader
        title="Discover"
        icon="i-lucide-radar"
      />
    </template>

    <template #body>
      <SnapshotGate v-slot="{ snapshot: workspace }">
        <form
          class="flex flex-col gap-3 rounded-lg border border-default p-4"
          aria-label="Scan for projects"
          data-testid="scan-form"
          @submit.prevent="scan"
        >
          <p class="text-sm text-muted">
            Look for mind projects, and git repositories that could be one, under the folders you name, as
            <code class="font-mono text-xs">flayer scan --suggest</code> does: what is most worth linking first. A folder is only read.
          </p>
          <div class="flex flex-wrap items-end gap-3">
            <UFormField
              label="Folders to look under"
              description="Absolute paths, one a line."
              name="roots"
              class="w-full sm:w-auto"
            >
              <UTextarea
                v-model="roots"
                :rows="2"
                autoresize
                autocomplete="off"
                spellcheck="false"
                size="sm"
                class="w-full font-mono sm:w-96"
                :disabled="busy"
                data-testid="scan-roots"
              />
            </UFormField>
            <UFormField
              label="Depth"
              description="Levels below each; empty for the default."
              name="depth"
            >
              <UInputNumber
                v-model="depth"
                :min="0"
                size="sm"
                class="w-32"
                :disabled="busy"
                data-testid="scan-depth"
              />
            </UFormField>
            <UButton
              type="submit"
              label="Scan"
              icon="i-lucide-scan-search"
              size="sm"
              :loading="isFetching"
              :disabled="busy || roots.trim() === ''"
              data-testid="scan"
            />
          </div>
        </form>

        <p
          v-if="!asked"
          class="text-muted"
          data-testid="not-asked"
        >
          Nothing is scanned until you ask: name the folders your repositories are kept in, then scan.
        </p>

        <UAlert
          v-else-if="refusal && !scanned"
          color="error"
          variant="subtle"
          icon="i-lucide-circle-alert"
          title="The scan was refused"
          :description="refusal"
          data-testid="scan-refused"
        />

        <p
          v-else-if="!scanned"
          role="status"
          class="text-muted"
          data-testid="scanning"
        >
          Scanning…
        </p>

        <template v-else>
          <p
            class="text-sm text-muted"
            data-testid="scanned"
          >
            Looked under
            <code
              v-for="root in scanned.roots"
              :key="root"
              class="me-1 font-mono text-xs"
            >{{ root }}</code>
            to depth {{ scanned.depth }}.
          </p>

          <ul
            v-if="scanned.warnings.length > 0"
            class="flex flex-col gap-1.5"
            aria-label="What the scan could not read"
            data-testid="scan-warnings"
          >
            <li
              v-for="(warning, index) in scanned.warnings"
              :key="index"
              class="flex items-start gap-2 text-sm"
              data-testid="scan-warning"
            >
              <UIcon
                name="i-lucide-circle-alert"
                class="mt-0.5 size-4 shrink-0 text-warning"
                aria-hidden="true"
              />
              <span class="min-w-0 break-words">
                <span
                  v-if="warning.code"
                  class="me-2 font-mono text-xs text-dimmed"
                >{{ warning.code }}<span class="sr-only">:</span></span>
                {{ warning.text }}
              </span>
            </li>
          </ul>

          <p
            v-if="scanned.found.length === 0"
            class="text-muted"
            data-testid="nothing-found"
          >
            No mind project or git repository under these folders.
          </p>

          <section
            v-if="moved.length > 0"
            aria-labelledby="moved-heading"
            class="flex flex-col gap-2"
            data-testid="moved"
          >
            <h2
              id="moved-heading"
              class="text-sm font-medium text-highlighted"
            >
              Moved
            </h2>
            <p class="text-sm text-muted">
              Each holds the id of a project an entry points at where nothing is any more: relinking follows the entry, and what was installed, to where it is now.
            </p>
            <ul class="flex flex-col divide-y divide-default rounded-lg border border-default">
              <li
                v-for="suggestion in moved"
                :key="suggestion.found.path"
                class="flex flex-wrap items-center justify-between gap-3 px-3 py-2"
                data-testid="moved-row"
              >
                <div class="min-w-0">
                  <div class="font-mono text-sm font-medium text-highlighted">
                    {{ suggestion.found.route }}
                  </div>
                  <div
                    class="text-xs text-muted"
                    data-testid="why"
                  >
                    {{ suggestion.why }}
                  </div>
                </div>
                <UButton
                  label="Relink"
                  icon="i-lucide-link-2"
                  size="sm"
                  color="neutral"
                  variant="outline"
                  :aria-label="`Relink ${suggestion.moved_from} to ${suggestion.found.route}`"
                  :disabled="busy"
                  data-testid="relink"
                  @click="relink(suggestion)"
                />
              </li>
            </ul>
          </section>

          <section
            v-if="offered.length > 0"
            aria-labelledby="offered-heading"
            class="flex flex-col gap-2"
            data-testid="offered"
          >
            <h2
              id="offered-heading"
              class="text-sm font-medium text-highlighted"
            >
              Not linked yet
            </h2>
            <ul class="flex flex-col divide-y divide-default rounded-lg border border-default">
              <li
                v-for="suggestion in offered"
                :key="suggestion.found.path"
                class="flex items-start gap-3 px-3 py-2"
                data-testid="suggestion"
              >
                <UCheckbox
                  :model-value="marked.includes(suggestion.found.path)"
                  :disabled="busy"
                  :ui="{ label: 'font-mono text-sm font-medium text-highlighted' }"
                  :label="suggestion.found.route"
                  data-testid="mark"
                  @update:model-value="mark(suggestion, $event)"
                >
                  <template #description>
                    <span class="flex flex-wrap items-center gap-2 text-xs">
                      <UBadge
                        :label="suggestion.found.reason"
                        color="neutral"
                        variant="soft"
                        size="sm"
                        class="font-mono"
                        data-testid="reason"
                      />
                      <span
                        class="text-muted"
                        data-testid="why"
                      >{{ suggestion.why }}</span>
                    </span>
                    <span
                      class="mt-1 flex items-center gap-1.5 text-xs text-dimmed"
                      data-testid="offers"
                    >
                      <UIcon
                        :name="OFFERS[suggestion.offers!]?.icon ?? 'i-lucide-circle-dot'"
                        class="size-3.5 shrink-0"
                        aria-hidden="true"
                      />
                      {{ OFFERS[suggestion.offers!]?.label ?? suggestion.offers }}
                    </span>
                  </template>
                </UCheckbox>
              </li>
            </ul>

            <UAlert
              v-if="writtenInto.length > 0"
              color="info"
              variant="subtle"
              icon="i-lucide-folder-plus"
              title="mind init writes into a repository first"
              :description="`${writtenInto.map(suggestion => suggestion.found.route).join(', ')}: not a mind project yet. mind init makes each one, a .mind kept out of git and empty skills/ and rules/, before it is linked. Every line is shown before it runs.`"
              data-testid="writes-into"
            />

            <UButton
              label="Link what is marked"
              icon="i-lucide-link"
              size="sm"
              class="self-start"
              :disabled="busy || picked.length === 0"
              data-testid="link-marked"
              @click="link"
            />
          </section>

          <div
            v-if="scanned.untraced.length > 0"
            class="flex flex-col items-start gap-2"
            data-testid="untraced"
          >
            <p class="text-sm text-muted">
              An entry here points where no project is any more, and the workspace never learned its id, so nothing found can be told to be it, moved. If you know where it went:
            </p>
            <CommandHint
              :commands="scanned.untraced.map(entry => ({ line: flayer(workspace.workspace, `relink ${entry} <PATH>`), does: 'follow it to where it moved' }))"
            />
          </div>

          <section
            v-if="linked.length > 0"
            aria-labelledby="linked-heading"
            class="flex flex-col gap-2"
            data-testid="linked"
          >
            <h2
              id="linked-heading"
              class="text-sm font-medium text-highlighted"
            >
              Linked already
            </h2>
            <ul class="flex flex-wrap gap-2">
              <li
                v-for="suggestion in linked"
                :key="suggestion.found.path"
              >
                <code
                  class="font-mono text-xs text-muted"
                  data-testid="linked-route"
                >{{ suggestion.found.route }}</code>
              </li>
            </ul>
          </section>
        </template>
      </SnapshotGate>
    </template>
  </UDashboardPanel>
</template>
