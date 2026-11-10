<script lang="ts">
	import type { Snippet } from 'svelte';

	let { title, description, actions }: { title: string; description?: string; actions?: Snippet } =
		$props();
</script>

<header class="mb-6 flex flex-wrap items-end justify-between gap-3">
	<div class="grid gap-1">
		<h1 class="text-2xl font-semibold tracking-tight">{title}</h1>
		{#if description}<p class="text-sm text-muted-foreground">{description}</p>{/if}
	</div>
	{#if actions}<div class="flex gap-2">{@render actions()}</div>{/if}
</header>
