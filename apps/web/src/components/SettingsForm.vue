<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { Agent } from '@/api/generated/Agent'
import type { ProjectSettings } from '@/api/generated/ProjectSettings'
import type { Share } from '@/api/generated/Share'
import type { Toggles } from '@/api/generated/Toggles'
import { useRunner } from '@/api/commands'

const props = defineProps<{
  /** The project's entry, which the server turns into its folder. */
  entry: string
  /** What its settings say now, as the snapshot read them. */
  settings: ProjectSettings
  /** Every agent core knows, in its order: a box for each. */
  agents: Agent[]
}>()

const runner = useRunner()
const { busy } = runner

/**
 * What each share mode means, said for whoever picks one: a line for every
 * word the generated `Share` holds, so a mode core adds does not compile
 * until it is said here, and which modes there are is still core's.
 */
const SHARES: Record<Share, string> = {
  local: 'kept out of git, for this checkout alone',
  shared: 'left for the team to commit'
}

/** What the project's settings say, as the toggles started. */
const was = computed<Toggles>(() => ({
  agents: props.settings.agents,
  share: props.settings.share,
  edit_tracked: props.settings.edit_tracked
}))

const ticked = ref<Agent[]>([])
const share = ref<Share>('local')
const editTracked = ref(false)

// Back to what the settings say whenever they change, a run of these
// toggles among what changes them: what is shown is the disk's, once the
// snapshot reads it again, never what was asked.
watch(was, (now) => {
  ticked.value = [...now.agents]
  share.value = now.share
  editTracked.value = now.edit_tracked
}, { immediate: true })

function tick(agent: Agent, on: boolean | 'indeterminate') {
  ticked.value = on === true ? [...ticked.value, agent] : ticked.value.filter(each => each !== agent)
}

/**
 * What the toggles say: the agents ticked, in core's order, which the task
 * module puts in the settings' own order before it names them.
 */
const now = computed<Toggles>(() => ({
  agents: props.agents.filter(agent => ticked.value.includes(agent)),
  share: share.value,
  edit_tracked: editTracked.value
}))

/** Whether anything was turned over: nothing is offered to run before. */
const touched = computed(() =>
  now.value.share !== was.value.share
  || now.value.edit_tracked !== was.value.edit_tracked
  || now.value.agents.length !== new Set(was.value.agents).size
  || now.value.agents.some(agent => !was.value.agents.includes(agent))
)

/**
 * Send the toggles as they stand, beside what the settings said: the task
 * module turns the two into the `agents` and `share` lines they amount to
 * (`Task::settings`), as the TUI's toggles, and the server shows those
 * before they run. Unticking every agent is sent too, and refused by the
 * command for its own reason.
 */
async function apply() {
  await runner.ask({ type: 'toggled', holder: { type: 'project', entry: props.entry }, was: was.value, now: now.value })
}
</script>

<template>
  <form
    class="flex flex-col gap-4 rounded-lg border border-default p-4"
    aria-labelledby="settings-heading"
    data-testid="settings-form"
    @submit.prevent="apply"
  >
    <div>
      <h2
        id="settings-heading"
        class="text-sm font-medium text-highlighted"
      >
        Agents and sharing
      </h2>
      <p class="text-sm text-muted">
        What is installed into the project is copied where each agent ticked reads it, and moved along when they change.
      </p>
    </div>

    <fieldset class="flex flex-col gap-1.5">
      <legend class="mb-1.5 font-mono text-xs text-muted">
        agents
      </legend>
      <div class="flex flex-wrap gap-4">
        <UCheckbox
          v-for="agent in agents"
          :key="agent"
          :model-value="ticked.includes(agent)"
          :label="agent"
          :disabled="busy"
          :ui="{ label: 'font-mono text-sm' }"
          :data-testid="`agent-${agent}`"
          @update:model-value="tick(agent, $event)"
        />
      </div>
    </fieldset>

    <URadioGroup
      v-model="share"
      legend="share"
      :items="Object.entries(SHARES).map(([value, description]) => ({ value, label: value, description }))"
      orientation="horizontal"
      :disabled="busy"
      :ui="{ legend: 'mb-1.5 font-mono text-xs font-normal text-muted', label: 'font-mono text-sm' }"
      data-testid="share-mode"
    />

    <UCheckbox
      v-model="editTracked"
      label="Edit files git tracks to reach an agent"
      description="AGENTS.md, CLAUDE.md or an MCP config, in a local project."
      :disabled="busy"
      data-testid="edit-tracked"
    />

    <UButton
      type="submit"
      label="Apply"
      icon="i-lucide-check"
      size="sm"
      class="self-start"
      :disabled="busy || !touched"
      data-testid="apply-settings"
    />
  </form>
</template>
