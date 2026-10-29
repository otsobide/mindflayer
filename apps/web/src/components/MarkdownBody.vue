<script setup lang="ts">
import type { FunctionalComponent } from 'vue'
import { rendered } from '@/markdown'

defineProps<{
  /** Markdown somebody else wrote, drawn from its tokens (src/markdown.ts). */
  text: string
}>()

// The elements `rendered` makes, drawn where this is: a component of their
// own, so they are made again only when the text changes.
const Drawn: FunctionalComponent<{ text: string }> = props => rendered(props.text)
Drawn.props = ['text']
</script>

<template>
  <div
    class="min-w-0 text-sm leading-relaxed text-default"
    data-testid="markdown"
  >
    <Drawn :text="text" />
  </div>
</template>
