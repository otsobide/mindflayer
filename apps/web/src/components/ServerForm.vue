<script lang="ts">
import type { Given } from '@/api/generated/Given'

/**
 * A program and an address, each as the form holds it while it is typed:
 * what the options of `add mcp` say, and nothing else, so every field is
 * one of an option's values. A secret is the name of the variable an
 * agent reads it from, and no field holds a secret's value, there being
 * no option that takes one.
 */
export interface Fields {
  transport: 'program' | 'address'
  command: string
  args: string[]
  env: [string, string][]
  secretEnv: string[]
  argEnv: string[]
  url: string
  events: boolean
  headers: [string, string][]
  secretHeaders: [string, string][]
  bearer: string
  urlVars: [string, string][]
}

/** The fields of a server nothing is said of yet: a program, its fields empty. */
export function blank(): Fields {
  return {
    transport: 'program',
    command: '',
    args: [],
    env: [],
    secretEnv: [],
    argEnv: [],
    url: '',
    events: false,
    headers: [],
    secretHeaders: [],
    bearer: '',
    urlVars: []
  }
}

/** The fields `given`, as core read a definition back, fills in: where a change starts from. */
export function filled(given: Given): Fields {
  const fields = blank()
  if (given.type === 'program') {
    return {
      ...fields,
      command: given.command,
      args: [...given.args],
      env: given.env.map(([key, value]) => [key, value]),
      secretEnv: [...given.secret_env],
      argEnv: [...given.arg_env]
    }
  }
  return {
    ...fields,
    transport: 'address',
    url: given.url,
    events: given.events,
    headers: given.headers.map(([key, value]) => [key, value]),
    secretHeaders: given.secret_headers.map(([header, variable]) => [header, variable]),
    bearer: given.bearer ?? '',
    urlVars: given.url_vars.map(([name, variable]) => [name, variable])
  }
}

/**
 * What the fields say, as a task carries it to `add mcp` or `edit`: the
 * transport chosen, and its fields as typed, a row for each value given.
 * Nothing is judged here, nor a row left out: what core refuses of it is
 * said in core's words, and the line runs what was typed.
 */
export function given(fields: Fields): Given {
  if (fields.transport === 'program') {
    return {
      type: 'program',
      command: fields.command,
      args: [...fields.args],
      env: fields.env.map(([key, value]) => [key, value]),
      secret_env: [...fields.secretEnv],
      arg_env: [...fields.argEnv]
    }
  }
  return {
    type: 'address',
    url: fields.url,
    events: fields.events,
    headers: fields.headers.map(([key, value]) => [key, value]),
    secret_headers: fields.secretHeaders.map(([header, variable]) => [header, variable]),
    // An empty field is no bearer token, as leaving `--bearer-env` out is.
    bearer: fields.bearer === '' ? null : fields.bearer,
    url_vars: fields.urlVars.map(([name, variable]) => [name, variable])
  }
}
</script>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { refDebounced } from '@vueuse/core'
import type { Draft } from '@/api/generated/Draft'
import type { Holder } from '@/api/generated/Holder'
import type { Task } from '@/api/transport'
import { useRunner } from '@/api/commands'
import { useJudged } from '@/api/draft'
import DraftVerdict from './DraftVerdict.vue'

const props = defineProps<{
  /** The server whose transport it changes; none, and it adds one. */
  editing?: string
  /** Where a change starts from, as core reads the definition back. */
  start?: Given
}>()

const emit = defineEmits<{
  /** It ran, and wrote: the server's name. */
  done: [name: string]
}>()

/** The workspace's own: what is added to, and changed in. */
const WORKSPACE: Holder = { type: 'workspace' }

const name = ref('')
const description = ref('')
const fields = ref<Fields>(props.start ? filled(props.start) : blank())

// Another server to change starts from what it holds.
watch(() => [props.editing, props.start] as const, ([, start]) => {
  fields.value = start ? filled(start) : blank()
})

/** What the line would be asked to write, as core is asked to judge it. */
const draft = computed<Draft | undefined>(() => {
  const server = given(fields.value)
  if (props.editing !== undefined) {
    return { type: 'edit_server', name: props.editing, server }
  }
  return name.value === '' && description.value === ''
    ? undefined
    : { type: 'add', kind: 'mcp', name: name.value, description: description.value, template: null, server }
})
// Judged once somebody stops typing for a moment, not at every key.
const settled = refDebounced(draft, 250)
const judged = useJudged(WORKSPACE, settled)
const judging = computed(() => judged.isPlaceholderData.value || JSON.stringify(settled.value) !== JSON.stringify(draft.value))

const runner = useRunner()
const { busy } = runner

/** The task the form sends: `add mcp`, or `edit` of the server it changes. */
const task = computed<Task>(() => {
  const server = given(fields.value)
  return props.editing !== undefined
    ? { type: 'edit_server', holder: WORKSPACE, name: props.editing, server }
    : { type: 'add', holder: WORKSPACE, kind: 'mcp', name: name.value, description: description.value, template: null, server }
})

/** Send it, shown before it runs; sent whatever core said, which the line says again when it refuses. */
async function send() {
  const named = props.editing ?? name.value
  const ran = await runner.ask(task.value)
  if (ran?.outcome.ok) {
    // Added, the form is ready for the next one; changed, it shows what
    // the server holds now, which the page hands it again.
    if (props.editing === undefined) {
      name.value = ''
      description.value = ''
      fields.value = blank()
    }
    emit('done', named)
  }
}
</script>

<template>
  <div class="grid gap-6 lg:grid-cols-[minmax(0,36rem)_20rem]">
    <form
      class="flex flex-col gap-4"
      :aria-label="editing === undefined ? 'The new server' : `How ${editing} is started`"
      data-testid="server-form"
      @submit.prevent="send"
    >
      <p class="text-sm text-muted">
        <template v-if="editing === undefined">
          One of the library's own, to install into projects: written by <code class="font-mono text-xs">mindweaver add mcp</code>
          from how an agent starts the server or where it reaches it.
        </template>
        <template v-else>
          Written again by <code class="font-mono text-xs">mindweaver edit mcp:{{ editing }}</code> from these, everything else its
          definition holds kept as it is.
        </template>
        Each line is shown before it runs, and judged as you type.
      </p>

      <template v-if="editing === undefined">
        <UFormField
          label="Name"
          description="The key each agent's config names it by: letters, digits, hyphens and underscores."
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
          description="What it is for, which every listing shows."
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
      </template>

      <URadioGroup
        v-model="fields.transport"
        legend="How an agent gets to it"
        orientation="horizontal"
        :items="[
          { value: 'program', label: 'a program it starts', description: '--command' },
          { value: 'address', label: 'an address it reaches', description: '--url' }
        ]"
        :disabled="busy"
        :ui="{ legend: 'mb-1.5 text-sm font-medium text-default', description: 'font-mono text-xs' }"
        data-testid="transport"
      />

      <template v-if="fields.transport === 'program'">
        <UFormField
          label="Command"
          description="The program, as the agent finds it on its PATH, or a path."
          name="command"
          required
        >
          <UInput
            v-model="fields.command"
            autocomplete="off"
            spellcheck="false"
            size="sm"
            class="w-full font-mono"
            :disabled="busy"
            data-testid="server-command"
          />
        </UFormField>

        <fieldset
          class="flex flex-col gap-2"
          data-testid="args"
        >
          <legend class="mb-1 text-sm font-medium">
            Arguments
          </legend>
          <p class="text-xs text-muted">
            What it is started with, in order, each as written.
          </p>
          <div
            v-for="(_, at) in fields.args"
            :key="at"
            class="flex gap-2"
          >
            <UInput
              v-model="fields.args[at]"
              :aria-label="`Argument ${at + 1}`"
              autocomplete="off"
              spellcheck="false"
              size="sm"
              class="w-full font-mono"
              :disabled="busy"
              data-testid="arg"
            />
            <UButton
              icon="i-lucide-x"
              color="neutral"
              variant="ghost"
              size="sm"
              :aria-label="`Remove argument ${at + 1}`"
              :disabled="busy"
              @click="fields.args.splice(at, 1)"
            />
          </div>
          <UButton
            label="Add an argument"
            icon="i-lucide-plus"
            color="neutral"
            variant="outline"
            size="xs"
            class="self-start"
            :disabled="busy"
            data-testid="add-arg"
            @click="fields.args.push('')"
          />
        </fieldset>

        <fieldset
          class="flex flex-col gap-2"
          data-testid="env"
        >
          <legend class="mb-1 text-sm font-medium">
            Settings in its environment
          </legend>
          <p class="text-xs text-muted">
            Each a variable and its value, written into the library as it is: no secret goes here, and one that looks like a token is refused.
          </p>
          <div
            v-for="(pair, at) in fields.env"
            :key="at"
            class="flex gap-2"
          >
            <UInput
              v-model="pair[0]"
              :aria-label="`Setting ${at + 1}, its variable`"
              autocomplete="off"
              spellcheck="false"
              size="sm"
              class="w-full font-mono"
              :disabled="busy"
              data-testid="env-key"
            />
            <UInput
              v-model="pair[1]"
              :aria-label="`Setting ${at + 1}, its value`"
              autocomplete="off"
              spellcheck="false"
              size="sm"
              class="w-full font-mono"
              :disabled="busy"
              data-testid="env-value"
            />
            <UButton
              icon="i-lucide-x"
              color="neutral"
              variant="ghost"
              size="sm"
              :aria-label="`Remove setting ${at + 1}`"
              :disabled="busy"
              @click="fields.env.splice(at, 1)"
            />
          </div>
          <UButton
            label="Add a setting"
            icon="i-lucide-plus"
            color="neutral"
            variant="outline"
            size="xs"
            class="self-start"
            :disabled="busy"
            data-testid="add-env"
            @click="fields.env.push(['', ''])"
          />
        </fieldset>

        <fieldset
          class="flex flex-col gap-2"
          data-testid="secret-env"
        >
          <legend class="mb-1 flex items-center gap-1.5 text-sm font-medium">
            <UIcon
              name="i-lucide-key-round"
              class="size-3.5"
              aria-hidden="true"
            />
            Secrets in its environment
          </legend>
          <p
            class="text-xs text-muted"
            data-testid="never-stored"
          >
            Each the name of the environment variable an agent reads the secret from when it starts the server, passed on under that name.
            No value is stored: not here, not in the library, and not in any agent's file.
          </p>
          <div
            v-for="(_, at) in fields.secretEnv"
            :key="at"
            class="flex gap-2"
          >
            <UInput
              v-model="fields.secretEnv[at]"
              :aria-label="`Secret ${at + 1}, the name of its variable`"
              autocomplete="off"
              spellcheck="false"
              size="sm"
              class="w-full font-mono"
              :disabled="busy"
              data-testid="secret-env-variable"
            />
            <UButton
              icon="i-lucide-x"
              color="neutral"
              variant="ghost"
              size="sm"
              :aria-label="`Remove secret ${at + 1}`"
              :disabled="busy"
              @click="fields.secretEnv.splice(at, 1)"
            />
          </div>
          <UButton
            label="Add a secret"
            icon="i-lucide-plus"
            color="neutral"
            variant="outline"
            size="xs"
            class="self-start"
            :disabled="busy"
            data-testid="add-secret-env"
            @click="fields.secretEnv.push('')"
          />
        </fieldset>

        <fieldset
          class="flex flex-col gap-2"
          data-testid="arg-env"
        >
          <legend class="mb-1 text-sm font-medium">
            Arguments read from a variable
          </legend>
          <p class="text-xs text-muted">
            Each the name of a variable whose value is given after the arguments above. No secret: a process list shows it.
          </p>
          <div
            v-for="(_, at) in fields.argEnv"
            :key="at"
            class="flex gap-2"
          >
            <UInput
              v-model="fields.argEnv[at]"
              :aria-label="`Argument read from a variable ${at + 1}, the name of the variable`"
              autocomplete="off"
              spellcheck="false"
              size="sm"
              class="w-full font-mono"
              :disabled="busy"
              data-testid="arg-env-variable"
            />
            <UButton
              icon="i-lucide-x"
              color="neutral"
              variant="ghost"
              size="sm"
              :aria-label="`Remove argument read from a variable ${at + 1}`"
              :disabled="busy"
              @click="fields.argEnv.splice(at, 1)"
            />
          </div>
          <UButton
            label="Add an argument read from a variable"
            icon="i-lucide-plus"
            color="neutral"
            variant="outline"
            size="xs"
            class="self-start"
            :disabled="busy"
            data-testid="add-arg-env"
            @click="fields.argEnv.push('')"
          />
        </fieldset>
      </template>

      <template v-else>
        <UFormField
          label="Address"
          description="Where the server is, with a part read from a variable written {NAME} where it goes. Nothing before its host: a credential goes in a secret header."
          name="url"
          required
        >
          <UInput
            v-model="fields.url"
            autocomplete="off"
            spellcheck="false"
            size="sm"
            class="w-full font-mono"
            :disabled="busy"
            data-testid="url"
          />
        </UFormField>

        <UCheckbox
          v-model="fields.events"
          label="Over server-sent events"
          description="--sse, which some agents no longer reach, rather than streamable HTTP"
          :disabled="busy"
          data-testid="events"
        />

        <fieldset
          class="flex flex-col gap-2"
          data-testid="headers"
        >
          <legend class="mb-1 text-sm font-medium">
            Headers
          </legend>
          <p class="text-xs text-muted">
            Each a header and its value, written into the library as it is: no secret goes here, and one that looks like a token is refused.
          </p>
          <div
            v-for="(pair, at) in fields.headers"
            :key="at"
            class="flex gap-2"
          >
            <UInput
              v-model="pair[0]"
              :aria-label="`Header ${at + 1}, its name`"
              autocomplete="off"
              spellcheck="false"
              size="sm"
              class="w-full font-mono"
              :disabled="busy"
              data-testid="header-key"
            />
            <UInput
              v-model="pair[1]"
              :aria-label="`Header ${at + 1}, its value`"
              autocomplete="off"
              spellcheck="false"
              size="sm"
              class="w-full font-mono"
              :disabled="busy"
              data-testid="header-value"
            />
            <UButton
              icon="i-lucide-x"
              color="neutral"
              variant="ghost"
              size="sm"
              :aria-label="`Remove header ${at + 1}`"
              :disabled="busy"
              @click="fields.headers.splice(at, 1)"
            />
          </div>
          <UButton
            label="Add a header"
            icon="i-lucide-plus"
            color="neutral"
            variant="outline"
            size="xs"
            class="self-start"
            :disabled="busy"
            data-testid="add-header"
            @click="fields.headers.push(['', ''])"
          />
        </fieldset>

        <fieldset
          class="flex flex-col gap-2"
          data-testid="secret-headers"
        >
          <legend class="mb-1 flex items-center gap-1.5 text-sm font-medium">
            <UIcon
              name="i-lucide-key-round"
              class="size-3.5"
              aria-hidden="true"
            />
            Secret headers
          </legend>
          <p
            class="text-xs text-muted"
            data-testid="never-stored"
          >
            Each a header, and the name of the environment variable its whole value is read from when an agent starts the server.
            No value is stored: not here, not in the library, and not in any agent's file.
          </p>
          <div
            v-for="(pair, at) in fields.secretHeaders"
            :key="at"
            class="flex gap-2"
          >
            <UInput
              v-model="pair[0]"
              :aria-label="`Secret header ${at + 1}, its name`"
              autocomplete="off"
              spellcheck="false"
              size="sm"
              class="w-full font-mono"
              :disabled="busy"
              data-testid="secret-header-key"
            />
            <UInput
              v-model="pair[1]"
              :aria-label="`Secret header ${at + 1}, the name of its variable`"
              autocomplete="off"
              spellcheck="false"
              size="sm"
              class="w-full font-mono"
              :disabled="busy"
              data-testid="secret-header-variable"
            />
            <UButton
              icon="i-lucide-x"
              color="neutral"
              variant="ghost"
              size="sm"
              :aria-label="`Remove secret header ${at + 1}`"
              :disabled="busy"
              @click="fields.secretHeaders.splice(at, 1)"
            />
          </div>
          <UButton
            label="Add a secret header"
            icon="i-lucide-plus"
            color="neutral"
            variant="outline"
            size="xs"
            class="self-start"
            :disabled="busy"
            data-testid="add-secret-header"
            @click="fields.secretHeaders.push(['', ''])"
          />
        </fieldset>

        <UFormField
          label="Bearer token, the name of its variable"
          description="Sent as Authorization: Bearer and the token, read from this environment variable. No value is stored: not here, not in the library, and not in any agent's file."
          name="bearer"
        >
          <UInput
            v-model="fields.bearer"
            autocomplete="off"
            spellcheck="false"
            size="sm"
            class="w-full font-mono"
            :disabled="busy"
            data-testid="bearer-variable"
          />
        </UFormField>

        <fieldset
          class="flex flex-col gap-2"
          data-testid="url-vars"
        >
          <legend class="mb-1 text-sm font-medium">
            Parts of the address read from a variable
          </legend>
          <p class="text-xs text-muted">
            Each the name the address writes between braces, and the variable it is read from. No secret: a log shows an address.
          </p>
          <div
            v-for="(pair, at) in fields.urlVars"
            :key="at"
            class="flex gap-2"
          >
            <UInput
              v-model="pair[0]"
              :aria-label="`Part of the address ${at + 1}, its name`"
              autocomplete="off"
              spellcheck="false"
              size="sm"
              class="w-full font-mono"
              :disabled="busy"
              data-testid="url-var-name"
            />
            <UInput
              v-model="pair[1]"
              :aria-label="`Part of the address ${at + 1}, the name of its variable`"
              autocomplete="off"
              spellcheck="false"
              size="sm"
              class="w-full font-mono"
              :disabled="busy"
              data-testid="url-var-variable"
            />
            <UButton
              icon="i-lucide-x"
              color="neutral"
              variant="ghost"
              size="sm"
              :aria-label="`Remove part of the address ${at + 1}`"
              :disabled="busy"
              @click="fields.urlVars.splice(at, 1)"
            />
          </div>
          <UButton
            label="Add a part of the address"
            icon="i-lucide-plus"
            color="neutral"
            variant="outline"
            size="xs"
            class="self-start"
            :disabled="busy"
            data-testid="add-url-var"
            @click="fields.urlVars.push(['', ''])"
          />
        </fieldset>
      </template>

      <UButton
        type="submit"
        :label="editing === undefined ? 'Add the server' : `Change ${editing}`"
        :icon="editing === undefined ? 'i-lucide-plus' : 'i-lucide-save'"
        size="sm"
        class="self-start"
        :disabled="busy || (editing === undefined && name === '')"
        :data-testid="editing === undefined ? 'add' : 'change'"
      />
    </form>

    <DraftVerdict
      :judged="settled ? judged.data.value : undefined"
      :pending="judging"
      :error="judged.error.value"
      :writer="editing === undefined ? 'mindweaver add mcp' : 'mindweaver edit'"
    />
  </div>
</template>
