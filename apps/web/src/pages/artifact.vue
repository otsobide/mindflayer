<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import type { Contents } from '@/api/generated/Contents'
import type { Held } from '@/api/generated/Held'
import { useContents, useFiles, type Named } from '@/api/artifact'
import { useSnapshot } from '@/api/snapshot'
import { first, useQueryChange } from '@/address'
import AgentBadges from '@/components/AgentBadges.vue'
import AppHeader from '@/components/AppHeader.vue'
import CommandHint, { flayer, making } from '@/components/CommandHint.vue'
import FileTree from '@/components/FileTree.vue'
import IssueList from '@/components/IssueList.vue'
import MarkdownBody from '@/components/MarkdownBody.vue'
import RemoveOffer from '@/components/RemoveOffer.vue'
import RenameForm from '@/components/RenameForm.vue'
import SnapshotGate from '@/components/SnapshotGate.vue'
import StateBadge from '@/components/StateBadge.vue'
import { holding } from '@/library'
import { editLink, libraryLink, projectLink } from '@/routes'

const route = useRoute()
const change = useQueryChange()

// Which of the library's own, and which of its files is open, are the
// address's (src/routes.ts, src/address.ts): `?kind=skill&name=review&file=
// references/checklist.md`.
const kind = computed(() => first(route.query.kind))
const name = computed(() => first(route.query.name))
const file = computed(() => first(route.query.file))

const { data } = useSnapshot()

/** The one the address names, as the snapshot lists it. */
const held = computed<Held | undefined>(() =>
  data.value?.library.artifacts.find(each => each.kind === kind.value && each.name === name.value)
)

/** How a read of its files names it: the workspace's own, by `kind:name`. */
const named = computed<Named | undefined>(() =>
  held.value ? { holder: { type: 'workspace' }, reference: `${held.value.kind}:${held.value.name}` } : undefined
)

const files = useFiles(named)
// The file it is read from, which its listing names, cut where core cuts it.
const manifest = useContents(named, () => files.data.value?.manifest)
// And whichever file was chosen from its listing, as the text it is.
const opened = useContents(named, file)

const title = computed(() => held.value?.name ?? name.value ?? 'Artifact')

function choose(path: string) {
  change({ file: path === file.value ? undefined : path })
}

/** Why a file's text is not shown, in words, with its size. */
function withheld(contents: Contents): string {
  return contents.withheld === 'binary'
    ? `It is not text: ${contents.size} bytes, not shown.`
    : `It holds ${contents.size} bytes, more than is shown.`
}
</script>

<template>
  <UDashboardPanel
    id="artifact"
    role="main"
    data-testid="artifact"
  >
    <template #header>
      <AppHeader
        :title="title"
        icon="i-lucide-file-text"
      >
        <UButton
          v-if="held"
          label="Edit"
          icon="i-lucide-file-pen-line"
          color="neutral"
          variant="outline"
          size="sm"
          :to="editLink(held.kind, held.name)"
          data-testid="edit"
        />
      </AppHeader>
    </template>

    <template #body>
      <SnapshotGate v-slot="{ snapshot }">
        <UBreadcrumb
          :items="[
            { label: 'Library', icon: 'i-lucide-library-big', to: held ? libraryLink(held.kind) : '/library' },
            { label: title }
          ]"
        />

        <div
          v-if="!held"
          class="flex flex-col items-start gap-3"
          data-testid="no-artifact"
        >
          <p class="text-muted">
            <template v-if="kind === undefined || name === undefined">
              No artifact is named: the address says no kind or no name.
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
                data-testid="kind"
              />
            </div>
            <p
              v-if="held.summary"
              class="text-muted"
              data-testid="summary"
            >
              {{ held.summary }}
            </p>
            <code
              v-for="target in held.targets"
              :key="target"
              class="break-all font-mono text-xs text-dimmed"
              data-testid="target"
            >{{ target }}</code>
          </div>

          <section
            aria-labelledby="issues-heading"
            class="flex flex-col gap-2"
            data-testid="issues"
          >
            <h2
              id="issues-heading"
              class="text-sm font-medium text-highlighted"
            >
              What validate says of it
            </h2>
            <p
              v-if="!held.unread && held.issues.length === 0"
              class="text-xs text-muted"
              data-testid="valid"
            >
              Nothing: it reads, and passes with no warning.
            </p>
            <div
              v-else
              class="text-xs"
            >
              <IssueList :held="held" />
            </div>
          </section>

          <section
            aria-labelledby="holding-heading"
            class="flex flex-col gap-2"
            data-testid="holding"
          >
            <h2
              id="holding-heading"
              class="text-sm font-medium text-highlighted"
            >
              The projects holding a copy of it
            </h2>
            <div
              v-if="holding(snapshot, held).length === 0"
              class="flex flex-col items-start gap-2"
              data-testid="no-holding"
            >
              <p class="text-xs text-muted">
                No project holds a copy of it yet.
              </p>
              <CommandHint
                :commands="[
                  ...making(snapshot.workspace),
                  { line: flayer(snapshot.workspace, `install -p <PROJECT> ${held.kind}:${held.name} --from ${snapshot.library.origin}`), does: 'into a project, from the library' }
                ]"
              />
            </div>
            <div
              v-else
              class="overflow-x-auto rounded-lg border border-default"
              tabindex="0"
              role="region"
              aria-label="The table of projects holding it"
            >
              <table class="w-full min-w-max text-left">
                <caption class="sr-only">
                  Each project holding a copy of it: how the copy stands, which of the project's agents read it, and where its copies are.
                </caption>
                <thead class="bg-elevated/40 font-mono text-xs text-muted">
                  <tr>
                    <th
                      scope="col"
                      class="px-3 py-2 font-medium"
                    >
                      project
                    </th>
                    <th
                      scope="col"
                      class="px-3 py-2 font-medium"
                    >
                      state
                    </th>
                    <th
                      scope="col"
                      class="px-3 py-2 font-medium"
                    >
                      agents
                    </th>
                    <th
                      scope="col"
                      class="px-3 py-2 font-medium"
                    >
                      targets
                    </th>
                  </tr>
                </thead>
                <tbody class="divide-y divide-default">
                  <tr
                    v-for="{ project, copy } in holding(snapshot, held)"
                    :key="project.entry ?? project.path"
                    class="align-top"
                    data-testid="holder"
                  >
                    <th
                      scope="row"
                      class="px-3 py-2 font-normal"
                    >
                      <RouterLink
                        :to="projectLink(project.entry ?? project.path)"
                        class="rounded-sm font-medium text-highlighted hover:text-primary focus-visible:outline-2 focus-visible:outline-primary"
                        data-testid="project"
                      >
                        {{ project.name ?? project.entry ?? project.path }}
                      </RouterLink>
                    </th>
                    <td class="px-3 py-2">
                      <StateBadge :state="copy.state" />
                    </td>
                    <td class="px-3 py-2">
                      <AgentBadges
                        :agents="project.agents"
                        :reached="copy.agents"
                      />
                    </td>
                    <td class="px-3 py-2">
                      <ul class="flex flex-col gap-0.5">
                        <li
                          v-for="target in copy.targets"
                          :key="target"
                        >
                          <code class="font-mono text-xs text-muted">{{ target }}</code>
                        </li>
                      </ul>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
          </section>

          <section
            aria-labelledby="front-matter-heading"
            class="flex flex-col gap-2"
            data-testid="front-matter"
          >
            <h2
              id="front-matter-heading"
              class="text-sm font-medium text-highlighted"
            >
              What its front matter declares
            </h2>
            <div
              v-if="manifest.data.value?.manifest?.front_matter"
              class="overflow-x-auto rounded-lg border border-default"
              tabindex="0"
              role="region"
              aria-label="The table of its front matter"
            >
              <table class="w-full text-left text-xs">
                <caption class="sr-only">
                  Each key its front matter holds, in the order written, and what it is given.
                </caption>
                <tbody class="divide-y divide-default">
                  <tr
                    v-for="(declaration, index) in manifest.data.value.manifest.front_matter"
                    :key="index"
                    class="align-top"
                    data-testid="declaration"
                  >
                    <th
                      scope="row"
                      class="w-40 px-3 py-1.5 font-mono font-medium text-muted"
                    >
                      {{ declaration.key }}
                    </th>
                    <td class="whitespace-pre-wrap break-all px-3 py-1.5 font-mono">
                      {{ declaration.value }}
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
            <p
              v-else-if="manifest.data.value?.manifest"
              class="text-xs text-muted"
              data-testid="no-front-matter"
            >
              Nothing core reads as front matter: all of it is what an agent is handed.
            </p>
            <p
              v-else-if="manifest.data.value"
              class="text-xs text-muted"
              data-testid="withheld"
            >
              {{ withheld(manifest.data.value) }}
            </p>
            <UAlert
              v-else-if="files.error.value ?? manifest.error.value"
              color="warning"
              variant="subtle"
              icon="i-lucide-file-x"
              title="Its file could not be read"
              :description="(files.error.value ?? manifest.error.value)?.message"
              data-testid="unread-manifest"
            />
            <p
              v-else
              role="status"
              class="text-xs text-muted"
            >
              Reading its file
            </p>
          </section>

          <section
            v-if="manifest.data.value?.manifest"
            aria-labelledby="body-heading"
            class="flex flex-col gap-2"
            data-testid="body"
          >
            <h2
              id="body-heading"
              class="text-sm font-medium text-highlighted"
            >
              What it says
            </h2>
            <div class="rounded-lg border border-default px-4 py-2">
              <MarkdownBody :text="manifest.data.value.manifest.body" />
            </div>
          </section>

          <section
            aria-labelledby="files-heading"
            class="flex flex-col gap-2"
            data-testid="tree"
          >
            <h2
              id="files-heading"
              class="text-sm font-medium text-highlighted"
            >
              Its files
            </h2>
            <FileTree
              v-if="files.data.value"
              :files="files.data.value"
              :selected="file"
              @select="choose"
            />
            <UAlert
              v-else-if="files.error.value"
              color="warning"
              variant="subtle"
              icon="i-lucide-folder-x"
              title="What it holds could not be listed"
              :description="files.error.value.message"
              data-testid="unlisted"
            />
            <p
              v-else
              role="status"
              class="text-xs text-muted"
            >
              Listing what it holds
            </p>

            <div
              v-if="file"
              class="flex flex-col gap-2"
              data-testid="opened"
            >
              <div class="flex flex-wrap items-center gap-2">
                <h3 class="font-mono text-xs font-medium text-highlighted">
                  {{ file }}
                </h3>
                <UButton
                  label="Close"
                  icon="i-lucide-x"
                  color="neutral"
                  variant="ghost"
                  size="xs"
                  @click="change({ file: undefined })"
                />
              </div>
              <pre
                v-if="typeof opened.data.value?.text === 'string'"
                class="max-h-[32rem] overflow-auto rounded-md border border-default bg-muted p-3 font-mono text-xs"
                tabindex="0"
                data-testid="text"
              >{{ opened.data.value.text }}</pre>
              <p
                v-else-if="opened.data.value"
                class="text-xs text-muted"
                data-testid="opened-withheld"
              >
                {{ withheld(opened.data.value) }}
              </p>
              <UAlert
                v-else-if="opened.error.value"
                color="warning"
                variant="subtle"
                icon="i-lucide-file-x"
                title="It could not be read"
                :description="opened.error.value.message"
                data-testid="opened-error"
              />
              <p
                v-else
                role="status"
                class="text-xs text-muted"
              >
                Reading it
              </p>
            </div>
          </section>

          <RenameForm
            :kind="held.kind"
            :name="held.name"
          />

          <RemoveOffer
            :kind="held.kind"
            :name="held.name"
          />
        </template>
      </SnapshotGate>
    </template>
  </UDashboardPanel>
</template>
