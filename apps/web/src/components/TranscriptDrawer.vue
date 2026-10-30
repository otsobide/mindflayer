<script setup lang="ts">
import { computed } from 'vue'
import { storeToRefs } from 'pinia'
import { useToast } from '@nuxt/ui/composables'
import { useTranscript } from '@/api/transcript'
import type { Ran } from '@/api/transport'
import { useShell } from '@/stores/shell'

const { transcriptOpen } = storeToRefs(useShell())
const { data, error } = useTranscript(transcriptOpen)
const toast = useToast()

/**
 * Every line the session's tasks are, in the order they ran: what copying
 * as a script copies, each line exactly as the server previewed it and ran
 * it, one to a line, and nothing the panel wrote of its own. Each runs as
 * one command pasted into a shell in the workspace's folder, which the
 * server quoted it for.
 */
const lines = computed(() => (data.value ?? []).flatMap(ran => ran.lines))

/** A task's lines, each with what its run produced, or none for a line a failure before it kept from running. */
function steps(ran: Ran) {
  return ran.lines.map((line, index) => ({ line, run: ran.outcome.runs[index] }))
}

async function copy() {
  const script = lines.value.map(line => `${line}\n`).join('')
  try {
    await navigator.clipboard.writeText(script)
    toast.add({
      title: lines.value.length === 1 ? 'Copied 1 line' : `Copied ${lines.value.length} lines`,
      description: 'Paste them into a shell in the workspace\'s folder to run them again.',
      color: 'success',
      icon: 'i-lucide-copy-check'
    })
  } catch (refused) {
    toast.add({
      title: 'The browser would not copy',
      description: `${refused instanceof Error ? refused.message : String(refused)}: select the lines and copy them by hand.`,
      color: 'error',
      icon: 'i-lucide-clipboard-x'
    })
  }
}
</script>

<template>
  <USlideover
    v-model:open="transcriptOpen"
    title="Transcript"
    description="Every task this session ran, oldest first, as lines you could run again in the workspace's folder."
    :ui="{ content: 'sm:max-w-xl' }"
  >
    <template #body>
      <p
        v-if="error"
        class="text-error"
        data-testid="transcript-unread"
      >
        The transcript could not be read: {{ error.message }}
      </p>
      <p
        v-else-if="!data"
        role="status"
        class="text-muted"
      >
        Reading the transcript
      </p>
      <p
        v-else-if="data.length === 0"
        class="text-muted"
        data-testid="transcript-empty"
      >
        Nothing has run yet this session.
      </p>
      <ol
        v-else
        class="space-y-4"
        data-testid="transcript"
      >
        <li
          v-for="(ran, index) in data"
          :key="index"
          class="space-y-2 rounded-lg border border-default p-3"
          data-testid="entry"
        >
          <div
            v-for="(step, at) in steps(ran)"
            :key="at"
            data-testid="step"
          >
            <div class="flex items-start justify-between gap-2">
              <code
                class="min-w-0 font-mono text-xs break-all text-highlighted"
                data-testid="line"
              ><span
                class="select-none text-dimmed"
                aria-hidden="true"
              >$ </span>{{ step.line }}</code>
              <UBadge
                :label="step.run ? (step.run.ok ? 'ok' : 'failed') : 'not run'"
                :color="step.run ? (step.run.ok ? 'success' : 'error') : 'neutral'"
                :icon="step.run ? (step.run.ok ? 'i-lucide-circle-check' : 'i-lucide-circle-x') : 'i-lucide-circle-dashed'"
                variant="subtle"
                size="sm"
                class="shrink-0 font-mono"
                data-testid="ok"
              />
            </div>
            <pre
              v-if="step.run && (step.run.stdout || step.run.stderr.length > 0 || step.run.error)"
              class="mt-1.5 overflow-x-auto rounded-md bg-muted px-3 py-2 font-mono text-xs whitespace-pre-wrap text-toned"
              data-testid="said"
            >{{ step.run.stdout }}<template
              v-for="(said, line) in step.run.stderr"
              :key="line"
            >{{ said.text }}
</template><template v-if="step.run.error">error: {{ step.run.error.message }}
</template></pre>
          </div>
        </li>
      </ol>
    </template>

    <template #footer>
      <div class="flex w-full items-center justify-between gap-2">
        <span class="text-xs text-muted">{{ lines.length === 1 ? '1 line' : `${lines.length} lines` }}</span>
        <UButton
          label="Copy as script"
          icon="i-lucide-copy"
          color="neutral"
          variant="outline"
          :disabled="lines.length === 0"
          data-testid="copy"
          @click="copy"
        />
      </div>
    </template>
  </USlideover>
</template>
