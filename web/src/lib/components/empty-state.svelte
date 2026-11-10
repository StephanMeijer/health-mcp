<script lang="ts">
	import type { Snippet } from 'svelte';

	/** What will appear here, and how to add the first one. */
	let { title, hint, action }: { title: string; hint: string; action?: Snippet } = $props();
</script>

<div class="grid place-items-center gap-2 rounded-lg border border-dashed p-10 text-center">
	<p class="font-medium">{title}</p>
	<p class="max-w-sm text-sm text-muted-foreground">{hint}</p>
	{#if action}<div class="mt-2">{@render action()}</div>{/if}
</div>
